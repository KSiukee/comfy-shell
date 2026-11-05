//! 提示词编译器（#81）：一句话 + 当前模型 → 按该模型方言写的完整提示词。
//!
//! 分工是这份模块存在的理由：**模型只管写内容，形状归这里**。
//! 语言、段落、长度、比例/尺寸混入、以及「用户写死的东西必须原样出现」都由本地规则收口，
//! 所以这一层不联网、可单测，模型换了行为也不会漂。
//!
//! 方言口径（每个档位实际吃的那一口）：
//! - `qwen21`（Qwen-Image 2.1）：中文长句，一段到底；原生双语，中文反而更准。
//! - `moss*` / `kromoss`（MOSS-Image 系）：英文散文 + 版面词（左右上下/前景背景），
//!   画面里的文字用双引号原样给出（官方 PE 也这么要求）。
//! - `flux3` / `flux3base` / 自定义工作流：英文自然语言，单一连续段落。
//!
//! 口径想改：只动 `dialect_of` 里那三行常量，别在调用处打补丁。

/// 模型家族（决定方言，不决定参数）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Qwen,
    Moss,
    Flux,
}

/// 输出语言。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Zh,
    En,
}

/// 一种方言：语言 + 长度区间 + 给人看的名字。
#[derive(Clone, Copy, Debug)]
pub struct Dialect {
    pub family: Family,
    /// 稳定 id，给界面/日志用
    pub id: &'static str,
    /// 中文名（「中文长句」）
    pub label: &'static str,
    pub lang: Lang,
    /// 下限（字/词）：低于它给一条「偏短」提示，不拦
    pub min_units: usize,
    /// 上限：超了就按句末截断
    pub max_units: usize,
}

const QWEN_PROFILES: &[&str] = &["qwen21"];
const MOSS_PROFILES: &[&str] = &["moss", "mossbase", "mossimagine", "mossedit", "kromoss"];

const QWEN_SYS: &str = include_str!("../prompts/compile_qwen.txt");
const MOSS_SYS: &str = include_str!("../prompts/compile_moss.txt");
const FLUX_SYS: &str = include_str!("../prompts/compile_flux.txt");

pub const REPAIR_SYS: &str = "你是提示词修补器：只把用户指定必须原样出现的片段插回提示词，\
其余文字一个字不改；不解释、不加标题、不用 markdown；只输出修补后的完整提示词。";

pub fn family_of(profile: &str) -> Family {
    let p = profile.trim().to_ascii_lowercase();
    if QWEN_PROFILES.contains(&p.as_str()) {
        Family::Qwen
    } else if MOSS_PROFILES.contains(&p.as_str()) {
        Family::Moss
    } else {
        // flux3 / flux3base / custom:* / 未知：按自然语言（最不容易写坏）
        Family::Flux
    }
}

pub fn dialect_of(profile: &str) -> Dialect {
    match family_of(profile) {
        Family::Qwen => Dialect {
            family: Family::Qwen,
            id: "qwen",
            label: "中文长句",
            lang: Lang::Zh,
            min_units: 120,
            max_units: 300,
        },
        Family::Moss => Dialect {
            family: Family::Moss,
            id: "moss",
            label: "排版散文",
            lang: Lang::En,
            min_units: 80,
            max_units: 200,
        },
        Family::Flux => Dialect {
            family: Family::Flux,
            id: "flux",
            label: "自然语言",
            lang: Lang::En,
            min_units: 100,
            max_units: 350,
        },
    }
}

pub fn system_prompt(f: Family) -> &'static str {
    match f {
        Family::Qwen => QWEN_SYS,
        Family::Moss => MOSS_SYS,
        Family::Flux => FLUX_SYS,
    }
}

// ---------------------------------------------------------------- 子模块
//
// compiler.rs 只留「方言 + 消息装配 + 自检」这层壳：
//   compiler_text.rs  —— 形状（计数/清洗/截断）
//   compiler_atoms.rs —— 必须原样保留的片段
// 拆开是为了每个文件都短、单测能各测各的。

#[path = "compiler_atoms.rs"]
mod atoms_mod;
#[path = "compiler_text.rs"]
mod text;

pub use atoms_mod::{atoms, missing};
pub use text::{cjk_ratio, clean, end_punct, fit, strip_ratio, units};

use self::text::find_cliches;

/// 引号对（两种语言的都要认）：抽原子和脱壳共用一张表。
pub(crate) const QUOTE_PAIRS: &[(char, char)] = &[
    ('"', '"'),
    ('“', '”'),
    ('「', '」'),
    ('『', '』'),
    ('《', '》'),
    ('‘', '’'),
];

/// 编译后的自检：把所有「不像这个方言」的地方列出来给用户看。
/// 只解释、不悄悄改——用户要能看到模型哪里没听话。
pub fn review(
    text: &str,
    d: &Dialect,
    miss: &[String],
    fit_note: Option<String>,
    stripped: &[String],
) -> Vec<String> {
    let mut notes: Vec<String> = Vec::new();
    let ratio = cjk_ratio(text);
    match d.lang {
        Lang::Zh if ratio < 0.30 => notes.push(format!(
            "输出不像中文（汉字只占 {:.0}%），{} 要的是中文长句",
            ratio * 100.0,
            d.label
        )),
        Lang::En if ratio > 0.35 => notes.push(format!(
            "输出混进了不少中文（汉字占 {:.0}%），{} 要的是英文散文",
            ratio * 100.0,
            d.label
        )),
        _ => {}
    }
    if !miss.is_empty() {
        notes.push(format!("漏了你写死的片段：{}", miss.join("、")));
    }
    if let Some(n) = fit_note {
        notes.push(n);
    }
    if !stripped.is_empty() {
        notes.push(format!(
            "去掉了混进正文的比例/尺寸：{}（那是参数，不是提示词）",
            stripped.join("、")
        ));
    }
    let cliches = find_cliches(text);
    if !cliches.is_empty() {
        notes.push(format!(
            "含画质套话：{}（这类词对画面没帮助，愿意的话手删）",
            cliches.join("、")
        ));
    }
    notes
}

// ---------------------------------------------------------------- 消息装配

/// 画布提示（只影响构图，不进正文）：从当前宽高猜一个能看懂的说法。
pub fn canvas_label(width: Option<i64>, height: Option<i64>) -> Option<String> {
    let (w, h) = (width?, height?);
    if w <= 0 || h <= 0 {
        return None;
    }
    let r = w as f64 / h as f64;
    let near = |a: f64| (r - a).abs() < 0.06;
    let name = if near(1.0) {
        "1:1 方形"
    } else if near(3.0 / 2.0) {
        "3:2 横构图"
    } else if near(2.0 / 3.0) {
        "2:3 竖构图"
    } else if near(16.0 / 9.0) {
        "16:9 宽银幕"
    } else if near(9.0 / 16.0) {
        "9:16 竖屏"
    } else if r > 1.25 {
        "横向画布"
    } else if r < 0.8 {
        "竖向画布"
    } else {
        "接近方形"
    };
    Some(format!("{name}（{w}×{h}）"))
}

/// 拼给模型的那条用户消息：先原话，再设定、画布、必须保留的片段。
pub fn user_message(
    idea: &str,
    ctx: Option<&str>,
    atoms: &[String],
    keep_wording: bool,
    canvas: Option<String>,
) -> String {
    let mut parts: Vec<String> = vec![
        "## 这句话想画的东西".into(),
        idea.trim().to_string(),
    ];
    if let Some(c) = ctx.map(str::trim).filter(|c| !c.is_empty()) {
        parts.push("## 这部戏的设定（照着写，别写反）".into());
        parts.push(c.to_string());
    }
    if let Some(c) = canvas {
        parts.push("## 画布".into());
        parts.push(format!("{c}——只影响构图，别把比例/尺寸写进提示词"));
    }
    if keep_wording && !atoms.is_empty() {
        parts.push("## 必须原样出现（漏一个都算没编好）".into());
        for a in atoms {
            parts.push(format!("- {a}"));
        }
    } else if !keep_wording {
        parts.push("## 措辞".into());
        parts.push("用户不在乎自己的原话，可以按你判断重写；但事实（谁、几个、写了什么字）不能改。".into());
    }
    parts.push("按上面的规则，只输出提示词正文。".into());
    parts.join("\n\n")
}

/// 修补消息：只插缺失片段，别的不许动。
pub fn repair_message(prompt: &str, miss: &[String]) -> String {
    let mut s = String::from("## 待修补的提示词\n");
    s.push_str(prompt.trim());
    s.push_str("\n\n## 现在缺的片段（原样插进合适的位置）\n");
    for m in miss {
        s.push_str(&format!("- {m}\n"));
    }
    s.push_str("\n只输出修补后的完整提示词，别解释。");
    s
}

/// 模型拒答/跑题时别把那段话当提示词。
pub fn looks_refused(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    if units(&t) < 12 {
        return true;
    }
    const NEEDLES: &[&str] = &[
        "i can't", "i cannot", "i'm sorry", "i am sorry", "cannot assist", "can't assist",
        "unable to", "as an ai", "抱歉", "对不起", "无法", "我不能", "作为一个人工智能",
    ];
    NEEDLES.iter().any(|n| t.contains(n))
}


#[cfg(test)]
#[path = "compiler_tests.rs"]
mod tests;
