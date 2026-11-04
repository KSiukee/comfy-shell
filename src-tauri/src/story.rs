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

fn preset_text(id: &str) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Story {
        Story {
            enabled: true,
            style: "guofeng".into(),
            style_custom: String::new(),
            era: "宋代江南，雨夜".into(),
            light: "低饱和冷调，暖黄灯笼光".into(),
            camera: "35mm 电影感，浅景深".into(),
            negative: "低清，畸形手".into(),
        }
    }

    #[test]
    fn composes_style_then_era_light_camera_then_raw() {
        let s = full();
        assert_eq!(
            s.compose("她回头看向巷口"),
            "国风工笔画，绢本质感，淡彩晕染，留白构图，宋代江南，雨夜，低饱和冷调，暖黄灯笼光，35mm 电影感，浅景深，她回头看向巷口"
        );
        assert_eq!(s.prefix(), "国风工笔画，绢本质感，淡彩晕染，留白构图，宋代江南，雨夜，低饱和冷调，暖黄灯笼光，35mm 电影感，浅景深");
    }

    #[test]
    fn disabled_is_a_verbatim_no_op() {
        let mut s = full();
        s.enabled = false;
        assert_eq!(s.compose("她回头看向巷口"), "她回头看向巷口");
        assert_eq!(s.compose("  她回头  "), "她回头");
        assert_eq!(s.prefix(), "");
    }

    #[test]
    fn empty_fields_are_skipped() {
        let s = Story { era: "  ".into(), light: String::new(), ..full() };
        let out = s.compose("x");
        assert!(out.ends_with("，x"));
        assert!(!out.contains("，，"), "空字段不能留下空逗号：{out}");
        assert_eq!(out, "国风工笔画，绢本质感，淡彩晕染，留白构图，35mm 电影感，浅景深，x");
    }

    #[test]
    fn custom_style_uses_the_free_text() {
        let s = Story { style: "custom".into(), style_custom: "像素画，16 色".into(), era: String::new(), light: String::new(), camera: String::new(), ..full() };
        assert_eq!(s.compose("x"), "像素画，16 色，x");
    }

    #[test]
    fn already_prefixed_text_is_not_doubled() {
        let s = full();
        let once = s.compose("x");
        assert_eq!(s.compose(&once), once);
    }

    #[test]
    fn empty_raw_returns_just_the_prefix() {
        assert_eq!(full().compose("  "), full().prefix());
    }

    #[test]
    fn presets_are_complete_and_ui_facing() {
        let p = presets();
        let arr = p.as_array().unwrap();
        assert!(arr.len() >= 8);
        assert!(arr.iter().any(|x| x["id"] == "photoreal" && !x["text"].as_str().unwrap().is_empty()));
        assert!(arr.iter().any(|x| x["id"] == "custom" && x["text"].as_str().unwrap().is_empty()));
        // 每个 id 都要能在 preset_text 里查到（custom 除外）
        for it in arr {
            let id = it["id"].as_str().unwrap();
            if id == "custom" {
                continue;
            }
            assert!(!preset_text(id).is_empty(), "预设 {id} 没有对应词");
        }
    }
}

#[cfg(test)]
mod lead_tests {
    use super::*;

    fn s() -> Story {
        Story {
            enabled: true,
            style: "photoreal".into(),
            style_custom: String::new(),
            era: String::new(),
            light: String::new(),
            camera: String::new(),
            negative: String::new(),
        }
    }

    #[test]
    fn lead_goes_after_the_style_prefix_and_before_the_panel() {
        let out = s().compose_with_lead(Some("她把伞收拢，退到檐下"), "她抬头看雨");
        assert!(out.ends_with("接上一格：她把伞收拢，退到檐下，她抬头看雨"));
        assert!(out.starts_with("写实摄影"));
    }

    #[test]
    fn empty_lead_is_skipped_without_extra_separators() {
        let out = s().compose_with_lead(Some("   "), "她抬头看雨");
        assert!(!out.contains("接上一格"));
        assert!(!out.contains("，，"));
        assert!(out.ends_with("她抬头看雨"));
    }

    #[test]
    fn disabled_story_drops_the_lead_too() {
        let mut st = s();
        st.enabled = false;
        assert_eq!(st.compose_with_lead(Some("上一格状态"), "这一格画面"), "这一格画面");
    }

    #[test]
    fn lead_is_not_doubled_when_already_present() {
        let once = s().compose_with_lead(Some("上一格状态"), "这一格画面");
        let twice = s().compose_with_lead(Some("上一格状态"), &once);
        assert_eq!(once, twice);
    }
}

#[cfg(test)]
mod split_ctx_tests {
    use super::*;

    fn s() -> Story {
        Story {
            enabled: true,
            style: "guofeng".into(),
            style_custom: String::new(),
            era: "宋代江南，雨夜".into(),
            light: "低饱和冷调".into(),
            camera: String::new(),
            negative: String::new(),
        }
    }

    #[test]
    fn context_lists_only_filled_fields_in_a_fixed_order() {
        let c = s().split_context();
        assert_eq!(c, "时代 / 地点：宋代江南，雨夜\n光线 / 色彩：低饱和冷调");
        // 画风词不进拆分上下文（否则会被写进每格正文，出图时重复）
        assert!(!c.contains("画风"));
        assert!(!c.contains("工笔"));
    }

    #[test]
    fn disabled_or_empty_context_is_empty() {
        let mut d = s();
        d.enabled = false;
        assert_eq!(d.split_context(), "");
        let e = Story { era: "  ".into(), light: String::new(), ..Story::default() };
        // 默认画风是 photoreal，所以这里只验证空字段不产生行
        assert!(!e.split_context().contains("时代"));
        assert!(!e.split_context().contains("光线"));
    }
}
