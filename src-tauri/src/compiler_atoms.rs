//! 编译器的「必须原样保留」层：从一句话里抽出用户写死的片段，并核对生成结果。
//!
//! 这一层决定「编译」和「扩写」的分界：扩写可以整段重写，编译不许丢你的字。

use super::text::units;
use super::QUOTE_PAIRS;

// ---------------------------------------------------------------- 必须原样保留的片段

/// 单位/量词：数字后面跟着这些才算「用户写死了一个数」。
const UNIT_CHARS: &[&str] = &[
    "只", "个", "位", "人", "张", "条", "匹", "栋", "层", "岁", "米", "秒", "天", "年", "月", "号",
    "分", "倍", "次", "件", "套", "双", "页", "格", "%", "％", "cm", "mm", "px", "kg", "ml", "k",
];

fn push_unique(out: &mut Vec<String>, s: &str) {
    let t = s.trim();
    if t.is_empty() {
        return;
    }
    if !out.iter().any(|x| x.eq_ignore_ascii_case(t)) {
        out.push(t.to_string());
    }
}

/// 用户写死、模型不许改写的片段：引号里的文字、数量、带大写/数字的拉丁专名。
///
/// 这是「编译」和「扩写」的分界线：扩写可以整段重写，编译不行。
pub fn atoms(idea: &str) -> Vec<String> {
    let chars: Vec<char> = idea.chars().collect();
    let mut out: Vec<String> = Vec::new();

    // 1) 引号里的文字（成对才算）
    let mut i = 0usize;
    while i < chars.len() {
        let mut hit = None;
        for (open, close) in QUOTE_PAIRS {
            if chars[i] == *open {
                let mut j = i + 1;
                while j < chars.len() && chars[j] != *close {
                    j += 1;
                }
                if j < chars.len() {
                    hit = Some((j, chars[i + 1..j].iter().collect::<String>()));
                }
                break;
            }
        }
        match hit {
            Some((j, inner)) => {
                if units(&inner) > 0 {
                    push_unique(&mut out, &inner);
                }
                i = j + 1;
            }
            None => i += 1,
        }
    }

    // 2) 数量：三位以上，或后面跟着量词/单位
    let mut i = 0usize;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            i += 1;
        }
        let num: String = chars[start..i].iter().collect();
        // 比例/尺寸（3:2、1024x768）不进原子：它们不该出现在正文里
        let next = chars.get(i).copied().unwrap_or(' ');
        if next == ':' || next == '：' || next == 'x' || next == '×' {
            continue;
        }
        let mut k = i;
        while k < chars.len() && chars[k] == ' ' {
            k += 1;
        }
        let tail: String = chars[k..(k + 2).min(chars.len())].iter().collect();
        let unit_follows = UNIT_CHARS.iter().any(|u| tail.starts_with(u));
        if num.chars().filter(|c| c.is_ascii_digit()).count() >= 3 || unit_follows {
            push_unique(&mut out, &num);
        }
    }

    // 3) 拉丁专名：带大写或数字、两个字符以上（iPhone / FLUX.3 / Cyberpunk2077）
    let mut cur = String::new();
    for c in chars.iter().copied() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '+' {
            cur.push(c);
        } else {
            take_latin(&mut out, &cur);
            cur.clear();
        }
    }
    take_latin(&mut out, &cur);
    prune_atoms(out)
}

/// 去掉被更长原子包含的碎片：引号里已经有「SPRING SALE 30%」，就别再单列 30 / SPRING——
/// 否则修补消息里排一长串重复项，模型反而更容易改坏。
///
/// 拉丁/数字按**词边界**判包含：「3 只猫」里的 3 不能被「SEN 30%」里的 3 吃掉。
fn prune_atoms(list: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in list {
        if out.iter().any(|k| frag_in(k, &a)) {
            continue;
        }
        out.retain(|k| !frag_in(&a, k));
        out.push(a);
    }
    out
}

fn frag_in(long: &str, short: &str) -> bool {
    let l: Vec<char> = norm(long).chars().collect();
    let s: Vec<char> = norm(short).chars().collect();
    if s.is_empty() || l == s || s.len() > l.len() {
        return false;
    }
    let cjk = s.iter().any(|c| ('\u{4e00}'..='\u{9fff}').contains(c));
    for i in 0..=(l.len() - s.len()) {
        if l[i..i + s.len()] != s[..] {
            continue;
        }
        if cjk {
            return true;
        }
        let before = if i == 0 { ' ' } else { l[i - 1] };
        let after = if i + s.len() >= l.len() { ' ' } else { l[i + s.len()] };
        if !before.is_alphanumeric() && !after.is_alphanumeric() {
            return true;
        }
    }
    false
}

fn take_latin(out: &mut Vec<String>, tok: &str) {
    let t = tok.trim_matches(|c: char| c == '.' || c == '-' || c == '_');
    if t.chars().count() < 3 {
        return;
    }
    if !t.chars().any(|c| c.is_ascii_digit()) && !t.chars().any(|c| c.is_ascii_uppercase()) {
        return;
    }
    if t.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return;
    }
    push_unique(out, t);
}

fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for c in s.to_lowercase().chars() {
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
    out.trim().to_string()
}

/// 生成结果里缺了哪些原子（大小写/空白不敏感，其余原样比）。
pub fn missing(prompt: &str, atoms: &[String]) -> Vec<String> {
    let hay = norm(prompt);
    atoms
        .iter()
        .filter(|a| !hay.contains(&norm(a)))
        .cloned()
        .collect()
}
