//! 提示词编译（#81）：一句话 + 当前模型 → 该模型方言的完整提示词。
//!
//! 三条边界，写在这里免得以后被顺手改掉：
//! - **只填提示词**：不改步数 / CFG / 尺寸 / 比例，也不动表单结构——填不填、填哪儿由用户点。
//! - **私密项目只走本地**：私密项目的字不出本机（红线见 docs/内置AI-功能想法-20261102.md §七）。
//! - **在线不行就回落本地**：没 key、引擎没装、模型拒答，都退到本机文本模型，并在结果里写清为什么。

use crate::*;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compiled {
    pub prompt: String,
    /// qwen / moss / flux
    pub family: String,
    /// 给人看的方言名（中文长句 / 排版散文 / 自然语言）
    pub dialect: String,
    /// zh / en
    pub lang: String,
    /// online / local
    pub source: String,
    pub notes: Vec<String>,
    pub atoms: Vec<String>,
    pub missing: Vec<String>,
    pub private: bool,
    /// 这次有没有把「这部戏的设定」一起发出去（界面上要如实说）
    pub used_context: bool,
    pub ms: u64,
}

/// 当前档位用哪套方言（界面在打开时就要显示，不必先编译一遍）。
#[tauri::command]
pub fn prompt_dialect(profile: String) -> serde_json::Value {
    let d = compiler::dialect_of(&profile);
    serde_json::json!({
        "family": d.id,
        "dialect": d.label,
        "lang": lang_id(d.lang),
        "minUnits": d.min_units,
        "maxUnits": d.max_units,
    })
}

fn lang_id(l: compiler::Lang) -> &'static str {
    match l {
        compiler::Lang::Zh => "zh",
        compiler::Lang::En => "en",
    }
}

#[tauri::command]
pub async fn prompt_compile(
    app: tauri::AppHandle,
    idea: String,
    profile: String,
    project: Option<String>,
    keep_wording: Option<bool>,
    width: Option<i64>,
    height: Option<i64>,
) -> Result<Compiled, String> {
    let idea = idea.trim().to_string();
    if idea.is_empty() {
        return Err("先写一句话想画什么".into());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        compile_sync(
            &handle,
            &idea,
            &profile,
            project.as_deref(),
            keep_wording.unwrap_or(true),
            width,
            height,
        )
    })
    .await
    .map_err(|e| format!("编译任务失败：{e}"))?
}

fn compile_sync(
    app: &tauri::AppHandle,
    idea: &str,
    profile: &str,
    project: Option<&str>,
    keep: bool,
    width: Option<i64>,
    height: Option<i64>,
) -> Result<Compiled, String> {
    let t0 = std::time::Instant::now();
    let cfg = cfg_dir(app);
    let s = app.state::<AppState>().settings.lock().unwrap().clone();
    let d = compiler::dialect_of(profile);
    let private = project.map(|p| vault::is_private(&cfg, p)).unwrap_or(false);
    let ctx = project
        .map(|p| story::load(&cfg, p).split_context())
        .filter(|c| !c.trim().is_empty());
    let atoms = if keep { compiler::atoms(idea) } else { Vec::new() };
    let user = compiler::user_message(
        idea,
        ctx.as_deref(),
        &atoms,
        keep,
        compiler::canvas_label(width, height),
    );
    let system = compiler::system_prompt(d.family);

    let mut notes: Vec<String> = Vec::new();
    if private {
        notes.push("私密项目：只走本机模型，不联网".into());
    }

    // 1) 在线优先（私密项目不给这条路）
    let mut text: Option<String> = None;
    let mut source = "local";
    let online_ready = !private && agent::has_key(&cfg) && agent::resolve_engine(&cfg, &s).found;
    if online_ready {
        match agent::run_text(&cfg, &s, system, &user, 90) {
            Ok(r) if !compiler::looks_refused(&r.text) => {
                text = Some(r.text);
                source = "online";
            }
            Ok(_) => notes.push("在线模型没给出可用提示词（疑似拒答），已改用本机模型".into()),
            Err(e) => notes.push(format!("在线调用没成（{e}），已改用本机模型")),
        }
    }

    // 2) 本地兜底
    let raw = match text {
        Some(t) => t,
        None => {
            if api::health(s.port).is_none() {
                return Err(if private {
                    "私密项目不走在线 AI，而 ComfyUI 没在运行：先启动 ComfyUI 再编译".into()
                } else {
                    "没有可用的编译引擎：先在设置里填 AI key（在线），或启动 ComfyUI（本地）".into()
                });
            }
            enhance::rewrite(s.port, system, &user, 700)?
        }
    };

    // 3) 交回编译器收口：清洗 → 去比例 → 长度 → 补标点
    let mut out = compiler::clean(&raw);
    let (stripped, removed) = compiler::strip_ratio(&out);
    out = stripped;
    let (fitted, fit_note) = compiler::fit(&out, &d);
    let mut fitted = compiler::end_punct(&fitted, d.lang);
    let mut fit_note = fit_note;

    // 4) 原子修补：只在线做一轮（本地模型上下文短，硬塞容易越修越乱）
    let mut miss = compiler::missing(&fitted, &atoms);
    if !miss.is_empty() && source == "online" {
        let msg = compiler::repair_message(&fitted, &miss);
        if let Ok(r) = agent::run_text(&cfg, &s, compiler::REPAIR_SYS, &msg, 60) {
            let cand = compiler::clean(&r.text);
            if !cand.trim().is_empty() && compiler::missing(&cand, &atoms).len() < miss.len() {
                let (again, note2) = compiler::fit(&cand, &d);
                fitted = compiler::end_punct(&again, d.lang);
                miss = compiler::missing(&fitted, &atoms);
                fit_note = note2;
                notes.push("漏掉的片段补了一轮".into());
            }
        }
    }

    notes.extend(compiler::review(&fitted, &d, &miss, fit_note, &removed));
    Ok(Compiled {
        prompt: fitted,
        family: d.id.to_string(),
        dialect: d.label.to_string(),
        lang: lang_id(d.lang).to_string(),
        source: source.to_string(),
        notes,
        atoms,
        missing: miss,
        private,
        used_context: !private && ctx.is_some(),
        ms: t0.elapsed().as_millis() as u64,
    })
}
