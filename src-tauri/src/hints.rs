//! 常用短语挖掘（拉的那一半）：从历史提示词里找出**你真的反复写的片段**。
//!
//! 只读 `history.json`（私密项目根本不写历史，所以锁区天然不参与）。
//! 这里不出任何句子给界面以外的用途——短语只在本机内存里算、在本机显示。

use crate::history::HistoryItem;
use serde::Serialize;
use std::collections::HashMap;

/// 最短/最长候选（字符数）。短于 3 的基本是"一个/画面"这类泛词，长于 12 的多半是整句。
const MIN_LEN: usize = 3;
const MAX_LEN: usize = 12;
/// 至少出现这么多次、并在至少这么多条提示词里出现过，才算"你的常用"。
const MIN_COUNT: u32 = 3;
const MIN_SOURCES: u32 = 2;
const CAP: usize = 40;

/// 泛词：出现频率高但没有信息量，学了只会占位置。
const STOPWORDS: &[&str] = &[
    "一个", "一位", "一只", "这个", "那个", "她的", "他的", "它们", "我们", "你们",
    "画面", "质感", "细节", "真实", "写实", "电影级", "高清", "精致", "光影", "氛围",
    "感觉", "看起来", "照片", "摄影", "效果", "风格", "人物", "角色", "背景", "前景",
    "女孩", "女生", "男孩", "少年", "少女", "男子", "女子", "模特",
];

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PhraseProject {
    pub id: String,
    pub count: u32,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Phrase {
    pub text: String,
    /// 出现次数（同一条提示词里重复也算）
    pub count: u32,
    /// 出现在几条提示词里
    pub sources: u32,
    pub last_ts: u64,
    /// 来自哪些项目（老记录没有项目字段 ⇒ id 为空串的那个）
    pub projects: Vec<PhraseProject>,
}

fn is_stopword(s: &str) -> bool {
    STOPWORDS.contains(&s)
}

/// 英文里的冠词/介词：短语不该以它们开头或结尾（「a woman」是泛词，「depth of field」里的 of 不算）。
const EN_EDGE: &[&str] = &[
    "a", "an", "the", "and", "of", "in", "on", "at", "with", "to", "for", "her", "his", "their",
    "is", "are", "as", "by", "from", "into",
];

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

/// 泛词/半泛词：本身是停用词、以停用词开头或结尾（「一个女孩」），或者整段都由泛词里的字组成
/// （「画面质感」）——这些学了只会占位置。英文按词判首尾。
fn is_generic(s: &str) -> bool {
    if is_stopword(s) {
        return true;
    }
    if !has_cjk(s) {
        let words: Vec<&str> = s.split_whitespace().collect();
        if words.len() < 2 {
            return true;
        }
        let edge = |w: &str| EN_EDGE.contains(&w.to_lowercase().as_str());
        return edge(words[0]) || edge(words[words.len() - 1]);
    }
    if STOPWORDS
        .iter()
        .any(|w| s != *w && (s.starts_with(w) || s.ends_with(w)))
    {
        return true;
    }
    let chars: Vec<char> = s.chars().collect();
    chars
        .iter()
        .all(|c| STOPWORDS.iter().any(|w| w.contains(*c)))
}

/// 把一条提示词切成候选片段：先按标点断句，再在每段里取 3..=12 字的滑窗。
fn segments(prompt: &str) -> Vec<String> {
    prompt
        .split(|c: char| "，,。.；;、\n\r\t|/（）()【】[]".contains(c))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn windows(seg: &str) -> Vec<String> {
    if !has_cjk(seg) {
        // 英文：按词取 2~4 个词的短语，别按字符切（会切出 "et " / "rai" 这种碎片）
        let words: Vec<&str> = seg.split_whitespace().filter(|w| !w.is_empty()).collect();
        let mut out = Vec::new();
        for n in 2..=4.min(words.len()) {
            for start in 0..=(words.len() - n) {
                out.push(words[start..start + n].join(" "));
            }
        }
        return out;
    }
    let chars: Vec<char> = seg.chars().collect();
    let mut out = Vec::new();
    let n = chars.len();
    if n < MIN_LEN {
        return out;
    }
    for len in MIN_LEN..=MAX_LEN.min(n) {
        for start in 0..=(n - len) {
            let w: String = chars[start..start + len].iter().collect();
            out.push(w);
        }
    }
    out
}

/// 挖短语。
///
/// - `project` 为 Some 时只看那个项目自己生成的记录（老记录没有项目字段，算"全局"）。
/// - `excluded` 里的项目**完全不参与**（不管它是明文、隐藏还是锁定项目，都按 id 排除）。
pub fn mine(
    items: &[HistoryItem],
    project: Option<&str>,
    hidden: &[String],
    excluded: &[String],
) -> Vec<Phrase> {
    let mut acc: HashMap<String, (u32, u32, u64, HashMap<String, u32>)> = HashMap::new();
    for item in items {
        if let Some(p) = project {
            if item.project.as_deref() != Some(p) {
                continue;
            }
        }
        let pid = item.project.clone().unwrap_or_default();
        if !pid.is_empty() && excluded.iter().any(|e| e == &pid) {
            continue;
        }
        // 一条提示词里出现几次（同一条里重复也算），以及它算不算"这条提示词里出现过"
        let mut occ: HashMap<String, u32> = HashMap::new();
        for seg in segments(&item.prompt) {
            for w in windows(&seg) {
                if is_generic(&w) || hidden.iter().any(|h| w.contains(h.as_str()) || h.contains(&w)) {
                    continue;
                }
                *occ.entry(w).or_insert(0) += 1;
            }
        }
        for (w, n) in occ {
            let e = acc.entry(w).or_insert_with(|| (0, 0, 0, HashMap::new()));
            e.0 += n; // 出现次数
            e.1 += 1; // 来源条数
            e.2 = e.2.max(item.ts);
            *e.3.entry(pid.clone()).or_insert(0) += n;
        }
    }

    let mut list: Vec<Phrase> = acc
        .into_iter()
        .filter(|(_, (c, s, _, _))| *c >= MIN_COUNT && *s >= MIN_SOURCES)
        .map(|(text, (count, sources, last_ts, by_project))| {
            let mut projects: Vec<PhraseProject> = by_project
                .into_iter()
                .map(|(id, count)| PhraseProject { id, count })
                .collect();
            projects.sort_by(|a, b| b.count.cmp(&a.count).then(a.id.cmp(&b.id)));
            Phrase { text, count, sources, last_ts, projects }
        })
        .collect();

    // 长的优先：被更长候选完全覆盖、且次数没明显更多的短词，就别占位置。
    list.sort_by(|a, b| b.text.chars().count().cmp(&a.text.chars().count()).then(b.count.cmp(&a.count)));
    let mut kept: Vec<Phrase> = Vec::new();
    for p in list {
        let covered = kept
            .iter()
            .any(|k| k.text.contains(&p.text) && k.count * 3 >= p.count * 2);
        if covered {
            continue;
        }
        kept.push(p);
    }
    kept.sort_by(|a, b| b.count.cmp(&a.count).then(b.last_ts.cmp(&a.last_ts)));
    kept.truncate(CAP);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(ts: u64, prompt: &str, project: Option<&str>) -> HistoryItem {
        HistoryItem {
            id: format!("h{ts}"),
            ts,
            profile: "moss".into(),
            label: "x".into(),
            mode: "t2i".into(),
            prompt: prompt.into(),
            negative: String::new(),
            width: 1024,
            height: 1024,
            steps: 50,
            cfg: 3.0,
            seed: 1,
            seconds: 1.0,
            images: Vec::new(),
            project: project.map(|s| s.to_string()),
        }
    }

    #[test]
    fn keeps_phrases_that_repeat_across_prompts() {
        let items = vec![
            item(1, "雨夜街头，浅景深电影感，一个女孩撑伞", None),
            item(2, "海边黄昏，浅景深电影感，单人剪影", None),
            item(3, "浅景深电影感，室内窗光", None),
            item(4, "只出现一次的特殊短语", None),
        ];
        let out = mine(&items, None, &[], &[]);
        let texts: Vec<&str> = out.iter().map(|p| p.text.as_str()).collect();
        assert!(texts.contains(&"浅景深电影感"), "重复三次的短语要挖出来：{texts:?}");
        assert!(!texts.iter().any(|t| t.contains("特殊短语")), "只出现一次的不能挖出来");
    }

    #[test]
    fn drops_stopwords_and_hidden() {
        let items = vec![
            item(1, "一个女孩，一个男孩，画面质感", None),
            item(2, "一个女孩，一个男孩，画面质感", None),
            item(3, "一个女孩，一个男孩，画面质感", None),
        ];
        let out = mine(&items, None, &[], &[]);
        assert!(out.is_empty(), "泛词全被停用词挡掉，实际：{:?}", out);

        let items2 = vec![
            item(1, "雨夜霓虹倒影，雨夜霓虹倒影", None),
            item(2, "雨夜霓虹倒影", None),
            item(3, "雨夜霓虹倒影", None),
        ];
        let out2 = mine(&items2, None, &["雨夜霓虹倒影".to_string()], &[]);
        assert!(out2.is_empty(), "手动隐藏的短语不该再出现");
    }

    #[test]
    fn project_filter_is_strict() {
        let items = vec![
            item(1, "项目A特有的短语组合", Some("a")),
            item(2, "项目A特有的短语组合", Some("a")),
            item(3, "项目A特有的短语组合", Some("a")),
            item(4, "项目A特有的短语组合", Some("b")),
            item(5, "项目A特有的短语组合", None),
        ];
        let only_a = mine(&items, Some("a"), &[], &[]);
        assert!(only_a.iter().any(|p| p.text == "项目A特有的短语组合"));
        let p = only_a.iter().find(|p| p.text == "项目A特有的短语组合").unwrap();
        assert_eq!(p.count, 3, "别的项目与老记录都不算进 a");
        let all = mine(&items, None, &[], &[]);
        let p2 = all.iter().find(|p| p.text == "项目A特有的短语组合").unwrap();
        assert_eq!(p2.count, 5);
        // 来源项目按次数排，老记录（无项目）的 id 是空串
        assert_eq!(p2.projects.len(), 3);
        assert_eq!(p2.projects[0].id, "a");
        assert_eq!(p2.projects[0].count, 3);
        assert!(p2.projects.iter().any(|x| x.id.is_empty() && x.count == 1));
    }

    #[test]
    fn excluded_projects_never_contribute() {
        let items = vec![
            item(1, "被排除项目里的常用短语", Some("secret")),
            item(2, "被排除项目里的常用短语", Some("secret")),
            item(3, "被排除项目里的常用短语", Some("secret")),
            item(4, "被排除项目里的常用短语", Some("ok")),
            item(5, "被排除项目里的常用短语", Some("ok")),
            item(6, "被排除项目里的常用短语", Some("ok")),
        ];
        let with = mine(&items, None, &[], &[]);
        assert_eq!(with[0].count, 6);
        let without = mine(&items, None, &[], &["secret".to_string()]);
        assert_eq!(without[0].count, 3, "被排除项目的 3 条不该算进来");
        assert!(without[0].projects.iter().all(|p| p.id != "secret"));
        // 剩下的不够门槛（只剩 2 条）⇒ 整条消失
        let gone = mine(&items[3..5], None, &[], &[]);
        assert!(gone.is_empty());
    }
}
