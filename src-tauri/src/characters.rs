//! 角色卡：把「身份 + 造型 + 风格」存成一份可复用的资产。
//!
//! 一个角色一个文件：`<配置目录>/characters/<id>.json`。`project` 为空 = 全局，
//! 否则只在那一个项目里出现（和配方库同一个口径）。
//!
//! 设计背景见 `设计笔记`：
//! - 一致性三层（身份 / 造型 / 风格）分开存；
//! - 真正管用的资产是 **identityPlate（3:4 全身身份底板）**，不是四视图板；
//! - 实测：FLUX.3 编辑从底板换装/换景，脸几乎不变（设计笔记）。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn dir(cfg: &Path) -> PathBuf {
    cfg.join("characters")
}

fn safe_id(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .take(60)
        .collect()
}

/// 一份造型（同一角色的第 N 套衣服）
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Outfit {
    #[serde(default)]
    pub version: String,
    /// 一句话造型描述（喂给编辑指令用）
    #[serde(default)]
    pub desc: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CharacterMeta {
    #[serde(default)]
    pub age: String,
    #[serde(default)]
    pub height: String,
    #[serde(default)]
    pub build: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Character {
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// None = 全局；Some(项目 id) = 只在那个项目里
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub meta: CharacterMeta,
    /// 身份锚点（中文，喂给模型的主体描述）
    #[serde(default)]
    pub identity_zh: String,
    /// 身份锚点（英文，可选）
    #[serde(default)]
    pub identity_en: String,
    #[serde(default)]
    pub palette: Vec<String>,
    #[serde(default)]
    pub outfits: Vec<Outfit>,
    /// 3:4 全身身份底板（本地图片路径或 ComfyUI 输出相对路径）
    #[serde(default)]
    pub identity_plate: Option<String>,
    /// 一致性档位：text / reference / lora
    #[serde(default)]
    pub consistency: String,
    /// 角色 LoRA 文件名（可选）
    #[serde(default)]
    pub lora: Option<String>,
    /// 最近一次一致性盲测结果，例如 "2026-11-01 7/10"
    #[serde(default)]
    pub graded: Option<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

impl Character {
    fn normalize(&mut self) -> Result<(), String> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err("角色名不能为空".into());
        }
        if name.chars().count() > 40 {
            return Err("角色名最多 40 字".into());
        }
        self.name = name;
        if self.identity_zh.chars().count() > 800 {
            return Err("身份描述最多 800 字".into());
        }
        if self.outfits.len() > 30 {
            return Err("造型最多 30 套".into());
        }
        let c = self.consistency.trim().to_lowercase();
        self.consistency = match c.as_str() {
            "" => "text".into(),
            "text" | "reference" | "lora" => c,
            _ => return Err("一致性档位只能是 text / reference / lora".into()),
        };
        if self.id.trim().is_empty() {
            self.id = format!(
                "{}-{}",
                now_ms(),
                safe_id(&self.name).chars().take(16).collect::<String>()
            );
        }
        let id = safe_id(&self.id);
        if id.is_empty() {
            return Err("角色 id 不合法".into());
        }
        self.id = id;
        Ok(())
    }
}

pub fn list(cfg: &Path) -> Vec<Character> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir(cfg)) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        if let Ok(c) = serde_json::from_str::<Character>(&text) {
            if !c.name.trim().is_empty() {
                out.push(c);
            }
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

pub fn save(cfg: &Path, mut c: Character) -> Result<Character, String> {
    c.normalize()?;
    if c.created_at <= 0 {
        c.created_at = now_ms() as i64;
    }
    c.updated_at = now_ms() as i64;
    std::fs::create_dir_all(dir(cfg)).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&c).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&dir(cfg).join(format!("{}.json", c.id)), &text)?;
    Ok(c)
}

pub fn delete(cfg: &Path, id: &str) -> Result<(), String> {
    let p = dir(cfg).join(format!("{}.json", safe_id(id)));
    if !p.is_file() {
        return Err("角色不存在".into());
    }
    std::fs::remove_file(p).map_err(|e| e.to_string())
}

/// 按项目过滤：`project` 为空时给全部；否则给「全局 + 该项目」。
pub fn visible(all: Vec<Character>, project: Option<&str>) -> Vec<Character> {
    match project {
        None => all,
        Some(p) => all
            .into_iter()
            .filter(|c| c.project.is_none() || c.project.as_deref() == Some(p))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        // 并行跑测试时毫秒级时间戳会撞车，加一个原子计数器保证每个用例一个目录
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!("cs_chars_{}_{}_{}", std::process::id(), n, nanos));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn base(name: &str) -> Character {
        Character {
            id: String::new(),
            name: name.into(),
            project: None,
            meta: CharacterMeta::default(),
            identity_zh: "乌黑长发，杏眼，红色花钿".into(),
            identity_en: "long black hair, almond eyes".into(),
            palette: vec!["奶白".into()],
            outfits: vec![Outfit { version: "v1".into(), desc: "白纱汉服".into() }],
            identity_plate: None,
            consistency: "text".into(),
            lora: None,
            graded: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn roundtrip_and_list() {
        let d = tmp();
        let c = save(&d, base("林小满")).unwrap();
        assert!(!c.id.is_empty());
        assert!(c.created_at > 0 && c.updated_at > 0);
        let all = list(&d);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].identity_zh, "乌黑长发，杏眼，红色花钿");
        assert_eq!(all[0].outfits.len(), 1);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn rejects_bad_input() {
        let d = tmp();
        assert!(save(&d, base("   ")).is_err(), "空名字要拒");
        let mut long = base("x");
        long.identity_zh = "字".repeat(801);
        assert!(save(&d, long).is_err(), "身份描述超长要拒");
        let mut bad = base("阿满");
        bad.consistency = "magic".into();
        assert!(save(&d, bad).is_err(), "档位只能是 text/reference/lora");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn project_visibility() {
        let d = tmp();
        let mut a = base("全局");
        a.project = None;
        save(&d, a).unwrap();
        let mut b = base("项目A");
        b.project = Some("proj-a".into());
        save(&d, b).unwrap();
        let mut c = base("项目B");
        c.project = Some("proj-b".into());
        save(&d, c).unwrap();

        let all = list(&d);
        assert_eq!(all.len(), 3);
        let only_a = visible(all.clone(), Some("proj-a"));
        let names: Vec<&str> = only_a.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"全局") && names.contains(&"项目A"));
        assert!(!names.contains(&"项目B"));
        assert_eq!(visible(all, None).len(), 3);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn delete_removes_file() {
        let d = tmp();
        let c = save(&d, base("阿满")).unwrap();
        assert!(delete(&d, &c.id).is_ok());
        assert!(list(&d).is_empty());
        assert!(delete(&d, &c.id).is_err(), "删两次要报错");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn corrupt_file_is_skipped() {
        let d = tmp();
        save(&d, base("好的")).unwrap();
        std::fs::create_dir_all(dir(&d)).unwrap();
        std::fs::write(dir(&d).join("broken.json"), "{ not json").unwrap();
        let all = list(&d);
        assert_eq!(all.len(), 1, "坏文件跳过，不许整个列表崩");
        let _ = std::fs::remove_dir_all(d);
    }
}
