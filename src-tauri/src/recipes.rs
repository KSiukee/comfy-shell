//! 配方库：把「模型 + LoRA + 参数 + 风格 + 排版 + 工作流参数」存成一份可复用的快照。
//!
//! 一份配方一个文件：`<配置目录>/recipes/<id>.json`。`project` 为空 = 全局可用，
//! 否则只在那个项目里出现。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn dir(cfg: &Path) -> PathBuf {
    cfg.join("recipes")
}

fn safe_id(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .take(60)
        .collect()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Recipe {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub note: String,
    /// None = 全局；Some(项目 id) = 只在那个项目里
    #[serde(default)]
    pub project: Option<String>,
    /// 表单快照（模型、尺寸、步数、LoRA、风格…由前端决定字段）
    pub form: Value,
    /// 编译后的排版 JSON（可选）
    #[serde(default)]
    pub layout: Option<String>,
    /// 工作流参数（可选）
    #[serde(default)]
    pub wf_params: Option<Value>,
    #[serde(default)]
    pub created_at: i64,
}

pub fn list(cfg: &Path) -> Vec<Recipe> {
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
        if let Ok(r) = serde_json::from_str::<Recipe>(&text) {
            if !r.name.trim().is_empty() {
                out.push(r);
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

pub fn save(cfg: &Path, mut r: Recipe) -> Result<Recipe, String> {
    let name = r.name.trim().to_string();
    if name.is_empty() {
        return Err("配方名称不能为空".into());
    }
    if name.chars().count() > 60 {
        return Err("配方名称最多 60 字".into());
    }
    if !r.form.is_object() {
        return Err("配方缺少表单快照".into());
    }
    r.name = name;
    if r.id.trim().is_empty() {
        r.id = format!("{}-{}", now_ms(), safe_id(&r.name).chars().take(16).collect::<String>());
    }
    let id = safe_id(&r.id);
    if id.is_empty() {
        return Err("配方 id 不合法".into());
    }
    r.id = id.clone();
    if r.created_at <= 0 {
        r.created_at = now_ms() as i64;
    }
    std::fs::create_dir_all(dir(cfg)).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&dir(cfg).join(format!("{id}.json")), &text)?;
    Ok(r)
}

pub fn delete(cfg: &Path, id: &str) -> Result<(), String> {
    let p = dir(cfg).join(format!("{}.json", safe_id(id)));
    if !p.is_file() {
        return Err("配方不存在".into());
    }
    std::fs::remove_file(p).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cs_recipes_{tag}_{}", std::process::id()))
    }

    fn sample(name: &str) -> Recipe {
        Recipe {
            id: String::new(),
            name: name.into(),
            note: String::new(),
            project: None,
            form: json!({"profile": "moss", "steps": 30}),
            layout: None,
            wf_params: None,
            created_at: 0,
        }
    }

    #[test]
    fn saves_lists_and_deletes() {
        let cfg = temp("crud");
        let _ = std::fs::remove_dir_all(&cfg);
        let saved = save(&cfg, sample("夜景电影感")).unwrap();
        assert!(!saved.id.is_empty());
        assert!(saved.created_at > 0);
        let all = list(&cfg);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "夜景电影感");
        assert_eq!(all[0].form["profile"], "moss");
        delete(&cfg, &saved.id).unwrap();
        assert!(list(&cfg).is_empty());
        assert!(delete(&cfg, &saved.id).is_err());
        let _ = std::fs::remove_dir_all(&cfg);
    }

    #[test]
    fn project_scope_is_kept() {
        let cfg = temp("scope");
        let _ = std::fs::remove_dir_all(&cfg);
        let mut r = sample("项目配方");
        r.project = Some("proj-1".into());
        let saved = save(&cfg, r).unwrap();
        assert_eq!(saved.project.as_deref(), Some("proj-1"));
        assert_eq!(list(&cfg)[0].project.as_deref(), Some("proj-1"));
        let _ = std::fs::remove_dir_all(&cfg);
    }

    #[test]
    fn rejects_bad_input() {
        let cfg = temp("bad");
        let _ = std::fs::remove_dir_all(&cfg);
        assert!(save(&cfg, sample("  ")).is_err());
        let mut r = sample("x");
        r.form = json!("不是对象");
        assert!(save(&cfg, r).is_err());
        let _ = std::fs::remove_dir_all(&cfg);
    }
}
