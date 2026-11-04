//! 剧本设定（#94）：一部戏的「长什么样」——画风、时代地点、光线色彩、镜头语言。
//!
//! 起因：剧情拆出来的每格只有一句 40~80 字的画面描述，生成时也只多挂了个
//! `@角色名`——画风/时代/光线全靠模型自由发挥，六张图能长出六个派系。
//! 这里把设定抽成一份**逐格复用**的前缀，拼在每格描述前面。
//!
//! 规矩：`enabled = false` 时 `compose()` 逐字返回原文（等于旧行为，一行回退）。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Story {
    pub enabled: bool,
    /// 画风预设 id；`custom` 时用 `style_custom`
    pub style: String,
    #[serde(default)]
    pub style_custom: String,
    #[serde(default)]
    pub era: String,
    #[serde(default)]
    pub light: String,
    #[serde(default)]
    pub camera: String,
    /// 统一负面词：格子自己没填时用它
    #[serde(default)]
    pub negative: String,
    /// #92 风格参考图的路径（只存路径；真正拼进提示词的是下面那句读出来的风格）
    #[serde(default)]
    pub style_ref: String,
    /// #92 从参考图读出来的画风句（色彩/光线/质感），可手改
    #[serde(default)]
    pub style_ref_text: String,
}

impl Default for Story {
    fn default() -> Self {
        Self {
            enabled: true,
            style: "photoreal".into(),
            style_custom: String::new(),
            era: String::new(),
            light: String::new(),
            camera: String::new(),
            negative: String::new(),
            style_ref: String::new(),
            style_ref_text: String::new(),
        }
    }
}

/// 画风预设：id / 下拉标签 / 实际拼进提示词的词。
/// UI 的下拉从 `story_presets` 拉，别在前端再抄一份（会走样）。
pub fn presets() -> Value {
    json!([
        {"id": "photoreal", "label": "写实摄影", "text": "写实摄影，真实皮肤质感，自然光影，电影剧照"},
        {"id": "guofeng", "label": "国风工笔", "text": "国风工笔画，绢本质感，淡彩晕染，留白构图"},
        {"id": "anime", "label": "日系动画", "text": "日系动画赛璐璐，干净线稿，平涂上色，动画截图"},
        {"id": "cg3d", "label": "3D 渲染", "text": "3D 渲染，次表面散射，柔和棚光，CG 角色"},
        {"id": "ink", "label": "水墨写意", "text": "水墨写意，墨分五色，宣纸质感，飞白笔触"},
        {"id": "cyber", "label": "赛博朋克", "text": "赛博朋克，霓虹冷暖对冲，湿润街面反光，高对比"},
        {"id": "film", "label": "胶片电影", "text": "35mm 胶片，颗粒感，轻微眩光，复古色调"},
        {"id": "custom", "label": "自定义", "text": ""},
    ])
}

pub(crate) fn preset_text(id: &str) -> &'static str {
    match id {
        "photoreal" => "写实摄影，真实皮肤质感，自然光影，电影剧照",
        "guofeng" => "国风工笔画，绢本质感，淡彩晕染，留白构图",
        "anime" => "日系动画赛璐璐，干净线稿，平涂上色，动画截图",
        "cg3d" => "3D 渲染，次表面散射，柔和棚光，CG 角色",
        "ink" => "水墨写意，墨分五色，宣纸质感，飞白笔触",
        "cyber" => "赛博朋克，霓虹冷暖对冲，湿润街面反光，高对比",
        "film" => "35mm 胶片，颗粒感，轻微眩光，复古色调",
        _ => "",
    }
}

impl Story {
    /// 这部戏的画风词：预设优先，`custom` 用自由文本。
    pub fn style_text(&self) -> String {
        if self.style == "custom" {
            self.style_custom.trim().to_string()
        } else {
            let t = preset_text(&self.style);
            if t.is_empty() {
                self.style_custom.trim().to_string()
            } else {
                t.to_string()
            }
        }
    }

    /// 只有前缀（给界面预览用）。
    pub fn prefix(&self) -> String {
        if !self.enabled {
            return String::new();
        }
        let mut parts: Vec<String> = Vec::new();
        let style = self.style_text();
        if !style.is_empty() {
            parts.push(style);
        }
        // #92：参考图读出来的画风句（比预设更贴用户手里的那张图）
        let ref_style = self.style_ref_text.trim();
        if !ref_style.is_empty() {
            parts.push(ref_style.to_string());
        }
        for p in [&self.era, &self.light, &self.camera] {
            let p = p.trim();
            if !p.is_empty() {
                parts.push(p.to_string());
            }
        }
        parts.join("，")
    }

    /// #91：给「拆镜头」用的设定块（拆的时候就要按这部戏的画风/时代写）。
    /// 关掉或全空时返回空串——拆分就退回旧行为。
    pub fn split_context(&self) -> String {
        if !self.enabled {
            return String::new();
        }
        // 只给「世界」不给「画风」：画风词一旦进了拆分上下文，模型会把它写进每格正文，
        // 出图时再拼一次前缀就是重复（实测 4 格里 1 格漏了「淡彩晕染出绢本质感」）。
        let mut lines: Vec<String> = Vec::new();
        for (label, v) in [("时代 / 地点", &self.era), ("光线 / 色彩", &self.light)] {
            let v = v.trim();
            if !v.is_empty() {
                lines.push(format!("{label}：{v}"));
            }
        }
        lines.join("\n")
    }

    /// #90：再带一句「接上一格：…」（上一格结束时的人物状态/位置/时间）。
    /// enabled = false 时连这句也一起撤掉（逐字回退）。
    pub fn compose_with_lead(&self, lead: Option<&str>, raw: &str) -> String {
        let raw = raw.trim();
        if !self.enabled {
            return raw.to_string();
        }
        let prefix = self.prefix();
        let lead = lead.map(|s| s.trim()).unwrap_or("");
        let lead_line = if lead.is_empty() {
            String::new()
        } else {
            format!("接上一格：{lead}")
        };
        // 已经拼过（用户手动粘过整段）就别再来一遍
        if !raw.is_empty() && raw.starts_with(&prefix) && (lead_line.is_empty() || raw.contains(&lead_line)) {
            return raw.to_string();
        }
        let mut parts: Vec<&str> = Vec::new();
        if !prefix.is_empty() {
            parts.push(&prefix);
        }
        if !lead_line.is_empty() && !raw.contains(&lead_line) {
            parts.push(&lead_line);
        }
        if !raw.is_empty() {
            parts.push(raw);
        }
        parts.join("，")
    }

    /// 把设定拼到这一格的画面描述前面。纯函数，边界都钉在单测里。
    pub fn compose(&self, raw: &str) -> String {
        let raw = raw.trim();
        if !self.enabled {
            return raw.to_string();
        }
        let prefix = self.prefix();
        if prefix.is_empty() {
            return raw.to_string();
        }
        if raw.is_empty() {
            return prefix;
        }
        // 已经带过前缀（用户手动粘过）就别再叠一层
        if raw.starts_with(&prefix) {
            return raw.to_string();
        }
        format!("{prefix}，{raw}")
    }
}

/// #96：清掉分镜描述里的「导演腔」——模型爱写"动作连贯推进""定格瞬间""冷暖对比"这类评论，
/// 它们对生图没有信息量，还会把画面往奇怪方向带。按分句过滤，全被过滤就原样返回（宁可留着也别清空）。
const META_PHRASES: &[&str] = &[
    "动作连贯", "连贯推进", "自然推进", "定格", "冷暖对比", "形成对比",
    "画面感", "镜头感", "视觉冲击", "叙事", "分镜", "转场", "景别变化", "构成一幅",
    "整体氛围", "情绪张力", "张力", "氛围感", "瞬间",
];

pub fn clean_panel(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let segs: Vec<&str> = trimmed.split(['，', '。', '；', '、']).collect();
    let kept: Vec<&str> = segs
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !META_PHRASES.iter().any(|m| s.contains(m)))
        .collect();
    if kept.is_empty() {
        return trimmed.to_string();
    }
    let mut out = kept.join("，");
    if trimmed.ends_with('。') {
        out.push('。');
    }
    out
}

fn file(cfg: &Path, project: &str) -> PathBuf {
    cfg.join("projects").join(project).join("story.json")
}

pub fn load(cfg: &Path, project: &str) -> Story {
    let p = file(cfg, project);
    std::fs::read_to_string(p)
        .ok()
        .and_then(|t| serde_json::from_str::<Story>(&t).ok())
        .unwrap_or_default()
}

pub fn save(cfg: &Path, project: &str, s: &Story) -> Result<(), String> {
    crate::project::load(cfg, project)?;
    if crate::vault::is_private(cfg, project) {
        return Err("私密项目暂不支持剧本设定".into());
    }
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&file(cfg, project), &text)
}
