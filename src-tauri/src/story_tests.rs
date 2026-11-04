//! 剧本设定的单测（从 story.rs 拆出来，免得模块自己变成上帝文件）。

use crate::story::*;

mod compose {
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
            style_ref: String::new(),
            style_ref_text: String::new(),
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
mod lead {
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
            style_ref: String::new(),
            style_ref_text: String::new(),
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
mod split_ctx {
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
            style_ref: String::new(),
            style_ref_text: String::new(),
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

#[cfg(test)]
mod style_ref {
    use super::*;

    #[test]
    fn ref_style_text_sits_after_the_preset() {
        let s = Story {
            enabled: true,
            style: "photoreal".into(),
            style_custom: String::new(),
            era: "宋代江南".into(),
            light: String::new(),
            camera: String::new(),
            negative: String::new(),
            style_ref: "/x/ref.png".into(),
            style_ref_text: "暖黄低饱和，窗边侧光，细腻颗粒".into(),
        };
        assert_eq!(
            s.prefix(),
            "写实摄影，真实皮肤质感，自然光影，电影剧照，暖黄低饱和，窗边侧光，细腻颗粒，宋代江南"
        );
        assert!(s.compose("她回头").ends_with("，她回头"));
    }

    #[test]
    fn empty_ref_text_adds_nothing() {
        let s = Story { style_ref_text: "   ".into(), ..Story::default() };
        assert_eq!(s.prefix(), "写实摄影，真实皮肤质感，自然光影，电影剧照");
    }
}
