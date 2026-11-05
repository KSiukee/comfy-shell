//! 看图拆提示词（#82）：把视觉模型那段 JSON 收成「五块 × 2~3 条候选短句」。
//!
//! 这一层只管**收口**，不管生图也不管界面：
//! - 容忍模型的花样（markdown 围栏、前后废话、单字符串、多余键、大小写）；
//! - 每条候选都收紧（去编号/标点、限长、去重），保证点一下填进提示词就是干净片段；
//! - 块不够（少于 3 块有内容）就明确报错，别把半份结果糊到界面上。

/// 五块的固定键与顺序：主体 / 镜头 / 光线 / 色彩 / 风格。
pub const BLOCK_KEYS: [&str; 5] = ["subject", "camera", "light", "color", "style"];
/// 每块最多留几条（多了按钮墙，少了没得挑）。
pub const MAX_PER_BLOCK: usize = 3;
/// 候选短句的上限：超过就按词截断（它该是片段，不是句子）。
pub const MAX_WORDS: usize = 12;
pub const MAX_CHARS: usize = 90;

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub key: &'static str,
    pub items: Vec<String>,
}

/// 解析模型输出。成功时按 `BLOCK_KEYS` 顺序返回 5 块（允许某块为空，但至少 3 块有内容）。
pub fn parse(raw: &str) -> Result<Vec<Block>, String> {
    let body = json_body(raw).ok_or_else(|| format!("没找到 JSON（{} 字）", raw.chars().count()))?;
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("JSON 解析失败：{e}"))?;
    let obj = value.as_object().ok_or("返回的不是对象")?;

    let mut blocks: Vec<Block> = Vec::new();
    for key in BLOCK_KEYS {
        let mut items: Vec<String> = Vec::new();
        // 大小写照收（Subject / SUBJECT 都认）
        for (k, v) in obj.iter() {
            if k.trim().eq_ignore_ascii_case(key) {
                collect(v, &mut items);
            }
        }
        items.truncate(MAX_PER_BLOCK);
        blocks.push(Block { key, items });
    }

    let filled = blocks.iter().filter(|b| !b.items.is_empty()).count();
    if filled < 3 {
        return Err(format!(
            "只认出 {filled} 块（要至少 3 块）——模型没按五块返回"
        ));
    }
    Ok(blocks)
}

/// 从一段可能带前言/围栏的文字里切出 JSON 对象本体。
fn json_body(raw: &str) -> Option<String> {
    let t = raw.trim();
    let t = t.trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let a = t.find('{')?;
    // 从最后一个 } 往回退，兼容「后面还有话」
    let b = t.rfind('}')?;
    if b <= a {
        return None;
    }
    let slice = &t[a..=b];
    // 先原文试一次；不行再试「把字符串里的裸换行/制表符转义」的版本
    if serde_json::from_str::<serde_json::Value>(slice).is_ok() {
        return Some(slice.to_string());
    }
    Some(escape_controls(slice))
}

fn escape_controls(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut in_str = false;
    let mut esc = false;
    for ch in s.chars() {
        if in_str {
            if esc {
                out.push(ch);
                esc = false;
                continue;
            }
            match ch {
                '\\' => {
                    out.push(ch);
                    esc = true;
                }
                '"' => {
                    out.push(ch);
                    in_str = false;
                }
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push(' '),
                _ => out.push(ch),
            }
        } else {
            if ch == '"' {
                in_str = true;
            }
            out.push(ch);
        }
    }
    out
}

/// 一个键下面可能是数组、单字符串，或 `{"items":[…]}`；都收成条目列表。
fn collect(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Array(arr) => {
            for x in arr {
                if let Some(s) = x.as_str() {
                    collect_str(out, s);
                }
            }
        }
        serde_json::Value::String(s) => collect_str(out, s),
        serde_json::Value::Object(o) => {
            for (k, x) in o.iter() {
                if k == "items" || k == "list" {
                    collect(x, out);
                }
            }
        }
        _ => {}
    }
}

/// 模型有时把几条候选塞进一个字符串（`a | b`），先拆再收。
fn collect_str(out: &mut Vec<String>, s: &str) {
    for part in s.split(['|', '；', ';']) {
        push_clean(out, part);
    }
}

fn push_clean(out: &mut Vec<String>, raw: &str) {
    let s = tidy(raw);
    if s.is_empty() || s.chars().count() > MAX_CHARS {
        return;
    }
    if !out.iter().any(|x| x.eq_ignore_ascii_case(&s)) {
        out.push(s);
    }
}

/// 去编号/项目符号/包裹引号/句末标点，压空白，按词截断。
pub fn tidy(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    // 模型爱加 "1. " / "- " / "• "
    loop {
        let before = s.clone();
        s = s
            .trim_start_matches(|c: char| c.is_whitespace() || c == '-' || c == '*' || c == '•' || c == '·')
            .to_string();
        let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            let rest = s[digits.len()..].trim_start();
            if let Some(r) = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')')).or_else(|| rest.strip_prefix('、')) {
                s = r.trim_start().to_string();
            }
        }
        if s == before {
            break;
        }
    }
    // 包裹引号（成对才脱）
    for (open, close) in [('"', '"'), ('“', '”'), ('「', '」'), ('\'', '\'')] {
        let n = s.chars().count();
        if n >= 2 && s.starts_with(open) && s.ends_with(close) {
            let inner: String = s.chars().skip(1).take(n - 2).collect();
            if !inner.contains(close) {
                s = inner.trim().to_string();
            }
            break;
        }
    }
    // 句末标点
    s = s.trim_end_matches(['.', '。', ',', '，', ';', '；', ':', '：']).trim().to_string();
    // 压空白
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(c);
            last_space = false;
        }
    }
    // 按词截断（超长就留前半，别把整条丢了）
    let words: Vec<&str> = out.split(' ').filter(|w| !w.is_empty()).collect();
    if words.len() > MAX_WORDS {
        return words[..MAX_WORDS].join(" ");
    }
    out.trim().to_string()
}

#[cfg(test)]
#[path = "vblocks_tests.rs"]
mod tests;
