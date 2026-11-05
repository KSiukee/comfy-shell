//! 编译器的「形状」层：计数、语言判定、清洗、去比例、长度收口、套话检测。
//!
//! 全是纯函数——不联网、不看模型是谁，只看文本长什么样。

use super::{Dialect, Lang, QUOTE_PAIRS};

/// 计数口径：汉字一个字算一个，拉丁词一个词算一个（中英混排也不失真）。
pub fn units(text: &str) -> usize {
    let mut cjk = 0usize;
    let mut words = 0usize;
    let mut in_word = false;
    for c in text.chars() {
        if ('\u{4e00}'..='\u{9fff}').contains(&c) {
            cjk += 1;
            in_word = false;
        } else if c.is_alphanumeric() {
            if !in_word {
                words += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }
    cjk + words
}

/// 汉字占「有效字符」的比例，用来看输出语言对不对。
pub fn cjk_ratio(text: &str) -> f32 {
    let mut cjk = 0usize;
    let mut all = 0usize;
    for c in text.chars() {
        if c.is_whitespace() || is_punct(c) {
            continue;
        }
        all += 1;
        if ('\u{4e00}'..='\u{9fff}').contains(&c) {
            cjk += 1;
        }
    }
    if all == 0 {
        0.0
    } else {
        cjk as f32 / all as f32
    }
}

/// 中英文标点都不算「有效字符」——否则中文句子的汉字占比会被逗号拉低。
fn is_punct(c: char) -> bool {
    c.is_ascii_punctuation() || "，。！？；：、“”‘’「」『』《》（）【】…—·".contains(c)
}
// ---------------------------------------------------------------- 清洗

const META_EN: &[&str] = &[
    "here is", "here's", "here are", "sure,", "sure:", "certainly", "of course", "below is",
    "the prompt:", "prompt:", "this prompt",
];
const META_ZH: &[&str] = &[
    "以下是", "下面是", "这是", "好的，", "好的:", "当然，", "当然：", "提示词：", "提示词:",
    "编译结果", "结果如下", "给你写", "给你一版",
];

fn is_meta_line(line: &str) -> bool {
    let l = line.trim();
    if l.is_empty() {
        return true;
    }
    let stripped = l.trim_start_matches(|c: char| {
        c.is_ascii_digit() || c == '.' || c == ')' || c == '）' || c == '、' || c == '-' || c == '*'
    });
    let low = stripped.to_lowercase();
    META_EN.iter().any(|m| low.starts_with(m)) || META_ZH.iter().any(|m| stripped.starts_with(m))
}

/// 只有标题没内容的一行（「提示词」「Prompt」）：后面还有正文时才丢。
fn is_heading_only(line: &str) -> bool {
    let l = line
        .trim()
        .trim_end_matches(['：', ':'])
        .trim()
        .to_lowercase();
    matches!(
        l.as_str(),
        "提示词" | "prompt" | "positive prompt" | "caption" | "描述" | "编译结果" | "结果"
    )
}

/// 模型爱在结尾加的客套话。
fn is_tail_line(line: &str) -> bool {
    let l = line.trim().to_lowercase();
    if l.is_empty() {
        return true;
    }
    const EN: &[&str] = &["hope this", "let me know", "feel free", "enjoy!", "this should work"];
    const ZH: &[&str] = &[
        "希望你喜欢", "希望这版", "希望这个", "需要我", "要不要我", "随时说", "可以继续", "需要的话",
    ];
    EN.iter().any(|m| l.starts_with(m)) || ZH.iter().any(|m| l.starts_with(m))
}

/// 去掉模型爱加的壳：前言、markdown、包裹引号、多余空行。
pub fn clean(raw: &str) -> String {
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let mut l = line.trim_end().to_string();
        let t = l.trim_start();
        // 代码围栏整行丢掉
        if t.starts_with("```") {
            continue;
        }
        // markdown 标题 / 项目符号去掉记号，内容留着
        if let Some(rest) = t.strip_prefix('#') {
            l = rest.trim_start_matches('#').trim().to_string();
        } else if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            l = rest.to_string();
        }
        lines.push(l);
    }
    // 只在「后面还有正文」时丢开头的前言行，避免把唯一一句也丢掉
    while lines.len() > 1 && (is_meta_line(&lines[0]) || is_heading_only(&lines[0])) {
        lines.remove(0);
    }
    // 结尾的客套话同理
    while lines.len() > 1 && is_tail_line(lines.last().map(|s| s.as_str()).unwrap_or("")) {
        lines.pop();
    }
    let mut out = lines.join("\n").trim().to_string();
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    // 整体被引号包住（内部没有同类引号）就脱掉
    for (open, close) in QUOTE_PAIRS {
        let n = out.chars().count();
        if n >= 2 && out.starts_with(*open) && out.ends_with(*close) {
            let inner: String = out.chars().skip(1).take(n - 2).collect();
            if !inner.contains(*close) {
                out = inner.trim().to_string();
            }
            break;
        }
    }
    out
}

/// 去掉混进正文的比例/尺寸（它们属于参数，不属于提示词），返回去掉的原文。
pub fn strip_ratio(text: &str) -> (String, Vec<String>) {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut removed: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let is_sep = c == ':' || c == '：' || c == 'x' || c == '×' || (c.is_whitespace() && c != '\n');
        if !is_sep {
            out.push(c);
            i += 1;
            continue;
        }
        // 往左看数字，往右看数字：两侧都是数字才算比例/尺寸
        let left_digits = out.chars().rev().take_while(|d| d.is_ascii_digit()).count();
        let mut j = i + 1;
        while j < chars.len() && chars[j].is_whitespace() && chars[j] != '\n' {
            j += 1;
        }
        let right = chars[j..].iter().take_while(|d| d.is_ascii_digit()).count();
        if left_digits > 0 && right > 0 {
            let left: String = out.chars().rev().take(left_digits).collect::<Vec<_>>().into_iter().rev().collect();
            let right_s: String = chars[j..j + right].iter().collect();
            let sep: String = chars[i..j].iter().collect();
            removed.push(format!("{left}{sep}{right_s}"));
            out.truncate(out.len() - left.len());
            i = j + right;
            continue;
        }
        out.push(c);
        i += 1;
    }
    let mut s = out;
    while s.contains("  ") {
        s = s.replace("  ", " ");
    }
    (s.trim().to_string(), removed)
}

/// 补一个句末标点（英文补 `.`，中文补 `。`）。
pub fn end_punct(text: &str, lang: Lang) -> String {
    ensure_end(text, lang)
}

fn ensure_end(text: &str, lang: Lang) -> String {
    let t = text.trim_end();
    if t.is_empty() {
        return t.to_string();
    }
    let last = t.chars().last().unwrap_or(' ');
    let ends = matches!(last, '.' | '!' | '?' | '。' | '！' | '？' | '"' | '”' | '」' | '』');
    if ends {
        return t.to_string();
    }
    match lang {
        Lang::Zh => format!("{t}。"),
        Lang::En => format!("{t}."),
    }
}

/// 按句末标点截断（找不到就按词/字切），尽量不切在半句中间。
fn truncate(text: &str, max: usize, lang: Lang) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut count = 0usize;
    let mut sentence_cut: Option<usize> = None;
    let mut comma_cut: Option<usize> = None;
    let mut in_word = false;
    let mut fallback = 0usize;
    let floor = max * 6 / 10;
    for (idx, c) in chars.iter().copied().enumerate() {
        if ('\u{4e00}'..='\u{9fff}').contains(&c) {
            count += 1;
            in_word = false;
        } else if c.is_alphanumeric() {
            if !in_word {
                count += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
        fallback = idx + 1;
        if count >= floor && matches!(c, '。' | '！' | '？' | '.' | '!' | '?') {
            sentence_cut = Some(idx + 1);
        } else if count >= floor && matches!(c, '，' | '；' | ',' | ';') {
            comma_cut = Some(idx + 1);
        }
        if count >= max {
            break;
        }
    }
    let cut = sentence_cut.or(comma_cut).unwrap_or(fallback);
    let mut s: String = chars.iter().take(cut).collect();
    if lang == Lang::En {
        // 英文别切在词中间
        if let Some(last) = s.chars().last() {
            if last.is_alphanumeric() {
                if let Some(pos) = s.rfind(|c: char| c.is_whitespace() || c == ',' || c == '.') {
                    s.truncate(pos);
                }
            }
        }
    }
    ensure_end(s.trim(), lang)
}

/// 长度收口：超上限按句末截断并给一条说明；低于下限只提示、不改。
pub fn fit(text: &str, d: &Dialect) -> (String, Option<String>) {
    let n = units(text);
    if n > d.max_units {
        let cut = truncate(text, d.max_units, d.lang);
        return (
            cut.clone(),
            Some(format!(
                "{} 习惯 {} 以内，这一版 {} → 已按句末截到 {}",
                d.label,
                d.max_units,
                n,
                units(&cut)
            )),
        );
    }
    if n < d.min_units {
        return (
            text.to_string(),
            Some(format!(
                "偏短：{} 字/词，{} 通常 {} 起；觉得单薄就补一句环境或光线",
                n, d.label, d.min_units
            )),
        );
    }
    (text.to_string(), None)
}

const CLICHES: &[&str] = &[
    "masterpiece", "best quality", "highly detailed", "ultra detailed", "8k", "4k", "uhd",
    "award winning", "award-winning", "trending on artstation", "sharp focus", "hyperrealistic",
    "杰作", "大师作品", "极致细节", "电影级光影", "超高清", "最佳质量",
];

pub(crate) fn find_cliches(text: &str) -> Vec<String> {
    let low = text.to_lowercase();
    let mut out = Vec::new();
    for c in CLICHES {
        if low.contains(c) {
            out.push((*c).to_string());
        }
    }
    out
}
