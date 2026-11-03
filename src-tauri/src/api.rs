//! Thin blocking client for the local ComfyUI HTTP API.
//!
//! Everything here talks to 127.0.0.1 only, so proxies are explicitly disabled
//! (the machine runs a system-wide Clash proxy that would otherwise swallow
//! localhost requests).

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn client(timeout_secs: u64) -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new())
}

fn base(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Returns the `/system_stats` payload when ComfyUI is answering.
pub fn health(port: u16) -> Option<Value> {
    let resp = client(2)
        .get(format!("{}/system_stats", base(port)))
        .send()
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<Value>().ok()
}

pub fn version(port: u16) -> Option<String> {
    let v = health(port)?;
    v.get("system")?
        .get("comfyui_version")?
        .as_str()
        .map(|s| s.to_string())
}

/// Query `/object_info/<node>` and return the node description.
pub fn node_info(port: u16, node: &str) -> Option<Value> {
    let resp = client(10)
        .get(format!("{}/object_info/{}", base(port), node))
        .send()
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().ok()?;
    v.get(node).cloned()
}

fn combo_options(info: &Value, input: &str, field: &str) -> Vec<String> {
    info.get("input")
        .and_then(|i| i.get(input))
        .and_then(|i| i.get(field))
        .and_then(|f| f.get(0))
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Serialize)]
pub struct Models {
    pub unets: Vec<String>,
    pub clips: Vec<String>,
    pub vaes: Vec<String>,
    pub loras: Vec<String>,
}

/// Lists the model files ComfyUI can see (filtered to what this shell uses).
pub fn models(port: u16) -> Models {
    let unets = node_info(port, "UNETLoader")
        .map(|i| combo_options(&i, "required", "unet_name"))
        .unwrap_or_default();
    let clips = node_info(port, "CLIPLoader")
        .map(|i| combo_options(&i, "required", "clip_name"))
        .unwrap_or_default();
    let vaes = node_info(port, "VAELoader")
        .map(|i| combo_options(&i, "required", "vae_name"))
        .unwrap_or_default();
    let loras = node_info(port, "LoraLoaderModelOnly")
        .map(|i| combo_options(&i, "required", "lora_name"))
        .unwrap_or_default();

    let pick = |v: Vec<String>, keys: &[&str]| -> Vec<String> {
        v.into_iter()
            .filter(|s| keys.iter().any(|k| s.contains(k)))
            .collect()
    };
    Models {
        unets: pick(unets, &["qwen_image", "flux3", "moss_image"]),
        clips: pick(clips, &["qwen3vl", "qwen3.5", "flux3", "heretic"]),
        vaes: pick(vaes, &["qwen_image", "flux2", "minimax"]),
        loras: pick(loras, &["qwen_image", "moss", "flux3", "minimax"]),
    }
}

// ---------------------------------------------------------------------------
// Generation request
// ---------------------------------------------------------------------------

fn slg_default_layers() -> String {
    "22".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenReq {
    pub profile: String,
    pub mode: String, // "t2i" | "edit"
    pub prompt: String,
    #[serde(default)]
    pub negative: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg: f64,
    #[serde(deserialize_with = "de_seed")]
    pub seed: i64,
    #[serde(default = "one")]
    pub batch: u32,
    #[serde(default)]
    pub lora: Option<String>,
    #[serde(default = "one_f")]
    pub lora_strength: f64,
    #[serde(default)]
    pub lora2: Option<String>,
    #[serde(default = "one_f")]
    pub lora2_strength: f64,
    #[serde(default)]
    pub ref_images: Vec<String>,
    #[serde(default)]
    pub slg: bool,
    #[serde(default = "slg_default_layers")]
    pub slg_layers: String,
    #[serde(default)]
    pub layout: Option<String>,
    /// 自定义工作流的参数覆盖：键 `节点id|输入名`
    #[serde(default)]
    pub wf_params: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    /// active project id: save under `output/projects/<id>/`
    #[serde(default)]
    pub project: Option<String>,
    /// 私密项目：先写 `_private_staging/`，任务结束后由壳加密封存
    #[serde(default)]
    pub staging: bool,
    /// 从哪条旧版本分叉（None = 接在最新版本后面）
    #[serde(default)]
    pub version_parent: Option<String>,
}

/// SaveImage prefix. Project ids are single path components; anything odd falls
/// back to the flat output root (never escape the output folder).
fn save_prefix(project: Option<&str>, staging: bool, base: &str) -> String {
    match project {
        Some(id) if !id.is_empty() && !id.contains(['/', '\\']) && id != "." && id != ".." => {
            if staging {
                format!("_private_staging/{id}/{base}")
            } else {
                format!("projects/{id}/{base}")
            }
        }
        _ => base.to_string(),
    }
}

fn one() -> u32 {
    1
}
fn one_f() -> f64 {
    1.0
}

pub struct Profile {
    pub unet: &'static str,
    pub clip: &'static str,
    pub clip_type: &'static str,
    pub vae: &'static str,
    pub sampler: &'static str,
    pub steps: u32,
    pub cfg: f64,
    pub turbo_lora: Option<&'static str>,
    pub turbo_steps: u32,
    pub supports_edit: bool,
    pub default_negative: &'static str,
    pub label: &'static str,
    pub shift: Option<f64>,
}

pub fn profile(name: &str) -> Option<Profile> {
    Some(match name {
        "qwen21" => Profile {
            unet: "qwen_image_2.1_int8_convrot.safetensors",
            clip: "qwen3vl_8b_int8_convrot_heretic.safetensors",
            clip_type: "qwen_image",
            vae: "qwen_image_2.1_vae_bf16.safetensors",
            sampler: "euler",
            steps: 45,
            // CFG 1.0 ignores the negative prompt entirely; Qwen needs ~4 to follow
            // "adult proportions" instead of drifting to its childlike prior.
            cfg: 4.0,
            turbo_lora: Some("qwen_image_2.1_turbo_lora_avg_rank_178_bf16.safetensors"),
            turbo_steps: 8,
            supports_edit: true,
            default_negative: "child, kid, toddler, loli, chibi, petite, short legs, \
                bad anatomy, deformed legs, extra limbs, doll-like proportions",
            label: "Qwen-Image 2.1",
            shift: None,
        },
        "flux3" => Profile {
            unet: "flux3-image-klein-int8-convrot.safetensors",
            clip: "flux3-klein-te-heretic-bf16.safetensors",
            clip_type: "flux3",
            vae: "flux2-vae.safetensors",
            sampler: "er_sde",
            steps: 16,
            cfg: 1.0,
            turbo_lora: None,
            turbo_steps: 16,
            supports_edit: true,
            default_negative: "",
            label: "FLUX.3 Klein（蒸馏）",
            shift: None,
        },
        "flux3base" => Profile {
            unet: "flux3-image-klein-base-int8-convrot.safetensors",
            clip: "flux3-klein-te-heretic-bf16.safetensors",
            clip_type: "flux3",
            vae: "flux2-vae.safetensors",
            sampler: "er_sde",
            steps: 50,
            cfg: 3.5,
            turbo_lora: None,
            turbo_steps: 50,
            supports_edit: true,
            default_negative: "",
            label: "FLUX.3 Klein Base（未蒸馏）",
            shift: None,
        },
        "moss" => Profile {
            unet: "moss_image_sft_int8_convrot.safetensors",
            clip: "moss_te_heretic_full_int8_convrot.safetensors",
            clip_type: "moss_image",
            vae: "flux2-vae_small.safetensors",
            sampler: "euler",
            steps: 50,
            cfg: 3.0,
            turbo_lora: None,
            turbo_steps: 25,
            supports_edit: false,
            default_negative: "nsfw, worst quality, bad anatomy, watermark, jpeg artifact",
            label: "MOSS-Image SFT",
            shift: None,
        },
        "mossimagine" => Profile {
            unet: "MOSS-Imagine-INT8-ConvRot.safetensors",
            clip: "moss_te_heretic_full_int8_convrot.safetensors",
            clip_type: "moss_image",
            vae: "flux2-vae_small.safetensors",
            sampler: "euler",
            steps: 50,
            cfg: 3.0,
            turbo_lora: None,
            turbo_steps: 50,
            supports_edit: false,
            default_negative: "nsfw, worst quality, bad anatomy, watermark, jpeg artifact",
            label: "MOSS-Imagine（真实感微调）",
            shift: None,
        },
        "mossbase" => Profile {
            unet: "moss_image_base_int8_convrot.safetensors",
            clip: "moss_te_heretic_full_int8_convrot.safetensors",
            clip_type: "moss_image",
            vae: "flux2-vae_small.safetensors",
            sampler: "euler",
            steps: 50,
            cfg: 3.0,
            turbo_lora: None,
            turbo_steps: 50,
            supports_edit: false,
            default_negative: "nsfw, worst quality, bad anatomy, watermark, jpeg artifact",
            label: "MOSS-Image Base（底模，可对比）",
            shift: None,
        },
        // MOSS 二编（ComfyUI 0.43 官方 MOSS-Image Edit 通路）：
        // Kromoss = MOSS-Image-Base 的全量微调（lodestones），配官方 8 步 DMD Turbo LoRA：
        // 1024² 实测 6 秒/张（MOSS SFT 50 步要 42-66 秒）。草稿档：细节比满步软。
        "kromoss" => Profile {
            unet: "kromoss_int8_convrot.safetensors",
            clip: "qwen3.5_9b_int8_convrot.safetensors",
            clip_type: "moss_image",
            vae: "flux2-vae_small.safetensors",
            sampler: "euler",
            steps: 8,
            cfg: 1.0,
            turbo_lora: None,
            turbo_steps: 8,
            supports_edit: false,
            default_negative: "nsfw, worst quality, bad anatomy, watermark, jpeg artifact",
            label: "Kromoss 8 步（MOSS 微调 · 约 6 秒/张）",
            shift: None,
        },
        // 文本编码器必须带 Qwen3.5 视觉权重，所以这里用官方的 qwen3.5_9b，
        // 不跟其他 MOSS 档共用 heretic 版。见 设计笔记
        "mossedit" => Profile {
            unet: "moss_image_sft_int8_convrot.safetensors",
            clip: "qwen3.5_9b_int8_convrot.safetensors",
            clip_type: "moss_image",
            vae: "flux2-vae_small.safetensors",
            sampler: "euler",
            steps: 30,
            cfg: 4.0,
            turbo_lora: None,
            turbo_steps: 30,
            supports_edit: true,
            default_negative: "nsfw, worst quality, bad anatomy, watermark, jpeg artifact",
            label: "MOSS-Image 编辑（指令改衣/改色/改光）",
            shift: None,
        },
        _ => return None,
    })
}

pub fn profiles_json() -> Value {
    let mut out = Vec::new();
    for name in ["qwen21", "flux3", "flux3base", "moss", "mossbase", "mossimagine", "mossedit", "kromoss"] {
        if let Some(p) = profile(name) {
            out.push(json!({
                "id": name,
                "label": p.label,
                "steps": p.steps,
                "cfg": p.cfg,
                "turboLora": p.turbo_lora,
                "turboSteps": p.turbo_steps,
                "supportsEdit": p.supports_edit,
                "defaultNegative": p.default_negative,
                // 工作流依赖检查用：设置页「工作流」一栏要能说出缺哪个模型
                "unet": p.unet,
                "clip": p.clip,
                "vae": p.vae,
                "sampler": p.sampler,
            }));
        }
    }
    Value::Array(out)
}

/// MOSS 二编用的编辑 LoRA（Comfy-Org 打包版，从 chinoll/Moss-Image-Edit 转来）。
pub const MOSS_EDIT_LORA: &str = "moss_image_edit_lora_rank_64_bf16.safetensors";

/// Kromoss 的 8 步 DMD Turbo LoRA（lodestones/Kromoss 官方配套）。
pub const KROMOSS_TURBO_LORA: &str = "kromoss_8steps_dmd_lora.safetensors";

/// Resolve the effective LoRA: explicit pick wins, otherwise the turbo LoRA when asked.
fn lora_node(model_src: &str, req: &GenReq, p: &Profile, turbo: bool) -> (Option<String>, String) {
    let picked = req
        .lora
        .clone()
        .or_else(|| if turbo { p.turbo_lora.map(|s| s.to_string()) } else { None });
    match picked {
        Some(name) => (Some(name), model_src.to_string()),
        None => (None, model_src.to_string()),
    }
}

fn seed_of(req: &GenReq) -> i64 {
    pick_seed(req.seed)
}

/// -1 (or any negative) means "roll a random seed".
///
/// #88：随机种子上限压在 2^53——再大就超出 JavaScript 的安全整数范围，图库/历史里
/// 显示和回填的种子会被舍入成另一个值，「用这张的参数再生成」就复现不了了。
pub fn pick_seed(seed: i64) -> i64 {
    if seed < 0 {
        use rand::Rng;
        const MAX_SAFE: i64 = 1i64 << 53;
        rand::thread_rng().gen_range(0..MAX_SAFE)
    } else {
        seed
    }
}

/// 种子的 JSON 形状：新数据是数字，前端为了不丢精度会回传字符串；两种都收。
pub fn de_seed<'de, D>(d: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    let v = Value::deserialize(d)?;
    match v {
        Value::Number(n) => n.as_i64().ok_or_else(|| D::Error::custom("seed 不是整数")),
        Value::String(s) => s
            .trim()
            .parse::<i64>()
            .map_err(|e| D::Error::custom(format!("seed 解析失败: {e}"))),
        _ => Err(D::Error::custom("seed 必须是整数或整数字符串")),
    }
}

/// 交给前端的种子一律用字符串：i64 超过 2^53 时 JSON 数字会被 JS 舍入。
pub fn seed_str<S>(v: &i64, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(&v.to_string())
}

/// Build an API-format graph for the requested profile/mode.
pub fn build_graph(req: &GenReq, turbo: bool) -> Result<Value, String> {
    let pos_text = req.layout.clone().unwrap_or_else(|| req.prompt.clone());
    let p = profile(&req.profile).ok_or_else(|| format!("未知模型配置: {}", req.profile))?;
    let steps = if turbo && p.turbo_lora.is_some() {
        p.turbo_steps
    } else {
        req.steps.max(1)
    };
    let cfg = if turbo && p.turbo_lora.is_some() {
        1.0
    } else {
        req.cfg
    };
    let batch = req.batch.clamp(1, 8);
    let mut g: BTreeMap<String, Value> = BTreeMap::new();

    g.insert(
        "1".into(),
        json!({"class_type": "UNETLoader", "inputs": {"unet_name": p.unet, "weight_dtype": "default"}}),
    );
    g.insert(
        "3".into(),
        json!({"class_type": "CLIPLoader", "inputs": {"clip_name": p.clip, "type": p.clip_type, "device": "default"}}),
    );
    g.insert("4".into(), json!({"class_type": "VAELoader", "inputs": {"vae_name": p.vae}}));

    // model chain: UNET -> [QwenImage21Cache] -> [LoRA]
    let mut model_src = "1".to_string();
    if req.profile == "qwen21" {
        g.insert(
            "2".into(),
            json!({"class_type": "QwenImage21Cache", "inputs": {"model": ["1", 0], "device": "auto", "dtype": "default"}}),
        );
        model_src = "2".into();
    }
    // Kromoss 的 8 步 Turbo LoRA 是必需件：不挂它，8 步 + CFG 1 会糊。
    // 所以写死在链路里，不靠「turbo」勾选框。
    if req.profile == "kromoss" {
        g.insert(
            "30".into(),
            json!({"class_type": "LoraLoaderModelOnly", "inputs": {
                "model": [model_src.clone(), 0], "lora_name": KROMOSS_TURBO_LORA, "strength_model": 1.0}}),
        );
        model_src = "30".into();
    }
    // MOSS 二编（官方 0.43 通路）：编辑 LoRA 先挂，再走 SLG 的 Simple 版。
    // 顺序照官方示例：UNET -> LoRA -> SkipLayerGuidanceDiTSimple -> KSampler。
    if req.profile == "mossedit" && !req.ref_images.is_empty() {
        g.insert(
            "31".into(),
            json!({"class_type": "LoraLoaderModelOnly", "inputs": {
                "model": [model_src.clone(), 0], "lora_name": MOSS_EDIT_LORA, "strength_model": 1.0}}),
        );
        g.insert(
            "32".into(),
            json!({"class_type": "SkipLayerGuidanceDiTSimple", "inputs": {
                "model": ["31", 0], "double_layers": "", "single_layers": req.slg_layers.clone(),
                "start_percent": 0.0, "end_percent": 1.0}}),
        );
        model_src = "32".into();
    }
    if let Some(shift) = p.shift {
        g.insert(
            "21".into(),
            json!({"class_type": "ModelSamplingSD3", "inputs": {"model": [model_src.clone(), 0], "shift": shift}}),
        );
        model_src = "21".into();
    }
    // MOSS official guidance: skip layer 22 on the negative branch (SLG)
    if req.slg && matches!(req.profile.as_str(), "moss" | "mossbase") {
        g.insert(
            "22".into(),
            json!({"class_type": "SkipLayerGuidanceDiT", "inputs": {
                "model": [model_src.clone(), 0], "double_layers": "", "single_layers": req.slg_layers.clone(),
                "scale": 3.0, "start_percent": 0.01, "end_percent": 1.0, "rescaling_scale": 0.0}}),
        );
        model_src = "22".into();
    }
    let (lora_name, _) = lora_node(&model_src, req, &p, turbo);
    let mut model_ref: Value = if let Some(name) = lora_name {
        g.insert(
            "11".into(),
            json!({"class_type": "LoraLoaderModelOnly",
                   "inputs": {"model": [model_src.clone(), 0], "lora_name": name,
                              "strength_model": req.lora_strength}}),
        );
        json!(["11", 0])
    } else {
        json!([model_src.clone(), 0])
    };

    // a second LoRA can be chained (both must belong to the same architecture)
    if let Some(name2) = req.lora2.clone().filter(|n| !n.is_empty()) {
        g.insert(
            "12".into(),
            json!({"class_type": "LoraLoaderModelOnly",
                   "inputs": {"model": model_ref, "lora_name": name2,
                              "strength_model": req.lora2_strength}}),
        );
        model_ref = json!(["12", 0]);
    }

    // Reference conditioning is not tied to the "edit" tab: Qwen-Image 2.1 and
    // FLUX.3 use the same weights for text-only and reference-guided generation,
    // so any attached reference switches the graph to the reference path.
    let edit = p.supports_edit && !req.ref_images.is_empty();

    // text encoding + latent
    let latent: Value = match (req.profile.as_str(), edit) {
        ("qwen21", false) => {
            g.insert(
                "5".into(),
                json!({"class_type": "TextEncodeQwenImage21", "inputs": {
                    "clip": ["3", 0], "prompt": req.prompt, "negative_prompt": req.negative,
                    "resolution": 1024}}),
            );
            g.insert(
                "7".into(),
                json!({"class_type": "EmptyLatentImage",
                       "inputs": {"width": req.width, "height": req.height, "batch_size": batch}}),
            );
            json!(["7", 0])
        }
        ("qwen21", true) => {
            let mut te = Map::new();
            te.insert("clip".into(), json!(["3", 0]));
            te.insert("prompt".into(), json!(req.prompt));
            te.insert("negative_prompt".into(), json!(req.negative));
            te.insert("resolution".into(), json!(req.width.max(req.height).clamp(512, 2048)));
            te.insert("vae".into(), json!(["4", 0]));
            for (i, img) in req.ref_images.iter().take(1).enumerate() {
                let nid = format!("{}", 20 + i);
                g.insert(nid.clone(), json!({"class_type": "LoadImage", "inputs": {"image": img}}));
                te.insert(format!("images.image_{}", i + 1), json!([nid, 0]));
            }
            g.insert("5".into(), json!({"class_type": "TextEncodeQwenImage21", "inputs": Value::Object(te)}));
            json!(["5", 2])
        }
        ("moss", _) => {
            g.insert("5".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["3", 0], "text": pos_text.clone()}}));
            let neg = if req.negative.is_empty() { p.default_negative } else { req.negative.as_str() };
            g.insert("6".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["3", 0], "text": neg}}));
            g.insert(
                "7".into(),
                json!({"class_type": "EmptyFlux2LatentImage",
                       "inputs": {"width": req.width, "height": req.height, "batch_size": batch}}),
            );
            json!(["7", 0])
        }
        // MOSS 二编：参考图既是「要改的那张图」，也编码成 reference latents。
        // edit_format 必须选 Moss-Image-Edit——那是 chinoll 这批 LoRA 训练用的
        // index_timestep_zero_frame0 布局；选错（比如默认的 ai-toolkit 布局）
        // 指令会完全失效、输出只是原图复刻（实测见 docs/58）。
        ("mossedit", true) => {
            let neg = if req.negative.is_empty() { p.default_negative } else { req.negative.as_str() };
            for (i, img) in req.ref_images.iter().take(4).enumerate() {
                g.insert(format!("{}", 20 + i), json!({"class_type": "LoadImage", "inputs": {"image": img}}));
            }
            for (nid, text) in [("5", pos_text.clone()), ("6", neg.to_string())] {
                let mut te = Map::new();
                te.insert("clip".into(), json!(["3", 0]));
                te.insert("vae".into(), json!(["4", 0]));
                te.insert("prompt".into(), json!(text));
                te.insert("edit_format".into(), json!("Moss-Image-Edit"));
                te.insert("image_strength".into(), json!(1.0));
                for i in 0..req.ref_images.len().min(4) {
                    te.insert(format!("images.image_{}", i), json!([format!("{}", 20 + i), 0]));
                }
                g.insert(nid.into(), json!({"class_type": "TextEncodeMOSSImageEdit", "inputs": Value::Object(te)}));
            }
            // MOSS 要求宽高都是 16 的倍数（镜头里常见的 1365 这种会被截到 1360）
            let w = (req.width / 16).max(1) * 16;
            let h = (req.height / 16).max(1) * 16;
            g.insert(
                "7".into(),
                json!({"class_type": "EmptyFlux2LatentImage",
                       "inputs": {"width": w, "height": h, "batch_size": batch}}),
            );
            json!(["7", 0])
        }
        ("mossedit", false) => {
            // 没挂参考图就是普通 MOSS 文生图（同一个底模、官方文本编码器）
            g.insert("5".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["3", 0], "text": pos_text.clone()}}));
            let neg = if req.negative.is_empty() { p.default_negative } else { req.negative.as_str() };
            g.insert("6".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["3", 0], "text": neg}}));
            let w = (req.width / 16).max(1) * 16;
            let h = (req.height / 16).max(1) * 16;
            g.insert(
                "7".into(),
                json!({"class_type": "EmptyFlux2LatentImage",
                       "inputs": {"width": w, "height": h, "batch_size": batch}}),
            );
            json!(["7", 0])
        }
        (_, false) => {
            g.insert("5".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["3", 0], "text": pos_text.clone()}}));
            g.insert("6".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["3", 0], "text": req.negative}}));
            g.insert(
                "7".into(),
                json!({"class_type": "EmptyFlux2LatentImage",
                       "inputs": {"width": req.width, "height": req.height, "batch_size": batch}}),
            );
            json!(["7", 0])
        }
        (_, true) => {
            // FLUX.3 edit: reference images ride through ReferenceMultiLatent.
            g.insert("5".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["3", 0], "text": pos_text.clone()}}));
            g.insert("6".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["3", 0], "text": req.negative}}));
            let mut rm = Map::new();
            rm.insert("positive".into(), json!(["5", 0]));
            rm.insert("negative".into(), json!(["6", 0]));
            rm.insert("vae".into(), json!(["4", 0]));
            if req.mode == "edit" {
                // editing keeps the source resolution so untouched pixels stay put
                rm.insert("resolution".into(), json!("keep resolution"));
            } else {
                // reference-guided generation: honour the size control's pixel area
                // (aspect ratio still follows the reference image)
                let mp = ((req.width as f64 * req.height as f64) / 1_000_000.0).clamp(0.1, 4.0);
                rm.insert("resolution".into(), json!("resize"));
                rm.insert("resolution.megapixels".into(), json!(mp));
            }
            for (i, img) in req.ref_images.iter().take(10).enumerate() {
                let nid = format!("{}", 20 + i);
                g.insert(nid.clone(), json!({"class_type": "LoadImage", "inputs": {"image": img}}));
                rm.insert(format!("images.image{}", i), json!([nid, 0]));
            }
            g.insert("7".into(), json!({"class_type": "ReferenceMultiLatent", "inputs": Value::Object(rm)}));
            json!(["7", 2])
        }
    };

    // positive / negative for the sampler
    // Qwen-Image 2.1 ships both in one TextEncodeQwenImage21 node; FLUX.3 edit
    // gets them back from ReferenceMultiLatent; everything else uses two text encodes.
    let (pos, neg): (Value, Value) = if req.profile == "qwen21" {
        (json!(["5", 0]), json!(["5", 1]))
    } else if edit && req.profile != "mossedit" {
        (json!(["7", 0]), json!(["7", 1]))
    } else {
        (json!(["5", 0]), json!(["6", 0]))
    };

    g.insert(
        "8".into(),
        json!({"class_type": "KSampler", "inputs": {
            "model": model_ref, "positive": pos, "negative": neg, "latent_image": latent,
            "seed": seed_of(req), "steps": steps, "cfg": cfg,
            "sampler_name": p.sampler, "scheduler": "simple", "denoise": 1.0}}),
    );
    g.insert("9".into(), json!({"class_type": "VAEDecode", "inputs": {"samples": ["8", 0], "vae": ["4", 0]}}));
    let prefix = save_prefix(req.project.as_deref(), req.staging, "comfyshell");
    g.insert(
        "10".into(),
        json!({"class_type": "SaveImage", "inputs": {"images": ["9", 0], "filename_prefix": prefix}}),
    );

    Ok(Value::Object(g.into_iter().map(|(k, v)| (k, v)).collect()))
}

// ---------------------------------------------------------------------------
// Job execution
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImageOut {
    pub filename: String,
    pub subfolder: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub data_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenOut {
    pub prompt_id: String,
    pub seconds: f64,
    pub images: Vec<ImageOut>,
}

pub struct Progress {
    pub elapsed: f64,
    pub queue_running: i64,
    pub queue_pending: i64,
    pub started: bool,
}

fn png_size(bytes: &[u8]) -> (u32, u32) {
    if bytes.len() > 24 && &bytes[1..4] == b"PNG" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return (w, h);
    }
    (0, 0)
}

/// Ask ComfyUI to unload models and release VRAM (before a long training run).
pub fn free_models(port: u16) {
    let _ = client(20)
        .post(format!("{}/free", base(port)))
        .json(&json!({"unload_models": true, "free_memory": true}))
        .send();
}

/// Submits the graph, waits for completion and returns the produced images.
pub fn run_job<F>(
    port: u16,
    comfy_root: &str,
    graph: Value,
    timeout_secs: u64,
    on_progress: F,
) -> Result<GenOut, String>
where
    F: FnMut(Progress),
{
    run_job_with(port, comfy_root, graph, timeout_secs, on_progress, true)
}

/// Same, but a graph that outputs no images (e.g. LoRA training) is fine.
pub fn run_job_any<F>(
    port: u16,
    comfy_root: &str,
    graph: Value,
    timeout_secs: u64,
    on_progress: F,
) -> Result<GenOut, String>
where
    F: FnMut(Progress),
{
    run_job_with(port, comfy_root, graph, timeout_secs, on_progress, false)
}

fn run_job_with<F>(
    port: u16,
    comfy_root: &str,
    graph: Value,
    timeout_secs: u64,
    mut on_progress: F,
    require_images: bool,
) -> Result<GenOut, String>
where
    F: FnMut(Progress),
{
    let start = Instant::now();
    let submit = client(30)
        .post(format!("{}/prompt", base(port)))
        .json(&json!({"prompt": graph, "client_id": "comfyshell"}))
        .send()
        .map_err(|e| format!("提交任务失败: {e}"))?;
    if !submit.status().is_success() {
        let txt = submit.text().unwrap_or_default();
        return Err(format!("ComfyUI 拒绝任务: {}", txt.chars().take(600).collect::<String>()));
    }
    let body: Value = submit.json().map_err(|e| format!("响应解析失败: {e}"))?;
    let prompt_id = body
        .get("prompt_id")
        .and_then(|v| v.as_str())
        .ok_or("ComfyUI 未返回 prompt_id")?
        .to_string();

    let hist_client = client(10);
    let queue_client = client(5);
    let mut started = false;
    loop {
        std::thread::sleep(Duration::from_millis(700));
        let elapsed = start.elapsed().as_secs_f64();
        if elapsed > timeout_secs as f64 {
            return Err(format!("超时（{timeout_secs}s）"));
        }
        if let Ok(resp) = hist_client.get(format!("{}/history/{}", base(port), prompt_id)).send() {
            if let Ok(hist) = resp.json::<Value>() {
                if let Some(entry) = hist.get(&prompt_id) {
                    let status = entry.get("status").cloned().unwrap_or(Value::Null);
                    let completed = status.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);
                    let status_str = status.get("status_str").and_then(|v| v.as_str()).unwrap_or("");
                    if status_str == "error" {
                        let msg = status
                            .get("messages")
                            .and_then(|m| m.as_array())
                            .and_then(|arr| arr.iter().find(|m| m.get(0).and_then(|x| x.as_str()) == Some("execution_error")))
                            .and_then(|m| m.get(1))
                            .and_then(|m| m.get("exception_message"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("执行失败");
                        return Err(format!("ComfyUI 执行失败: {msg}"));
                    }
                    if completed {
                        let mut images = Vec::new();
                        if let Some(outputs) = entry.get("outputs").and_then(|o| o.as_object()) {
                            for (_, out) in outputs {
                                if let Some(arr) = out.get("images").and_then(|i| i.as_array()) {
                                    for im in arr {
                                        let filename = im.get("filename").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        let subfolder = im.get("subfolder").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        if filename.is_empty() {
                                            continue;
                                        }
                                        // history entries are either saved outputs or temp previews
                                        let kind = im.get("type").and_then(|v| v.as_str()).unwrap_or("output");
                                        let mut path = PathBuf::from(comfy_root);
                                        path.push("ComfyUI");
                                        path.push(if kind == "temp" { "temp" } else { "output" });
                                        if !subfolder.is_empty() {
                                            path.push(&subfolder);
                                        }
                                        path.push(&filename);
                                        let bytes = match std::fs::read(&path) {
                                            Ok(b) => b,
                                            // temp previews are deleted by the time history is polled
                                            Err(_) => continue,
                                        };
                                        let (w, h) = png_size(&bytes);
                                        let data_url = format!(
                                            "data:image/png;base64,{}",
                                            base64::engine::general_purpose::STANDARD.encode(&bytes)
                                        );
                                        images.push(ImageOut {
                                            filename,
                                            subfolder,
                                            path: path.to_string_lossy().to_string(),
                                            width: w,
                                            height: h,
                                            data_url,
                                        });
                                    }
                                }
                            }
                        }
                        if images.is_empty() && require_images {
                            return Err("任务完成但没有产出图片".into());
                        }
                        return Ok(GenOut {
                            prompt_id,
                            seconds: start.elapsed().as_secs_f64(),
                            images,
                        });
                    }
                }
            }
        }

        if !started {
            if let Ok(resp) = queue_client.get(format!("{}/queue", base(port))).send() {
                if let Ok(q) = resp.json::<Value>() {
                    let running = q.get("queue_running").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0) as i64;
                    let pending = q.get("queue_pending").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0) as i64;
                    started = running > 0 || pending == 0;
                    on_progress(Progress {
                        elapsed,
                        queue_running: running,
                        queue_pending: pending,
                        started,
                    });
                }
            }
        } else {
            on_progress(Progress {
                elapsed,
                queue_running: 1,
                queue_pending: 0,
                started: true,
            });
        }
    }
}

pub fn interrupt(port: u16) {
    let _ = client(5)
        .post(format!("{}/interrupt", base(port)))
        .json(&json!({}))
        .send();
}

/// Upscale a file that already sits in ComfyUI/input (used by external-folder mode).
pub fn upscale_from_input(
    port: u16,
    comfy_root: &str,
    input_name: &str,
    up_model: &str,
) -> Result<ImageOut, String> {
    let graph = json!({
        "1": {"class_type": "LoadImage", "inputs": {"image": input_name}},
        "2": {"class_type": "UpscaleModelLoader", "inputs": {"model_name": up_model}},
        "3": {"class_type": "ImageUpscaleWithModel",
              "inputs": {"upscale_model": ["2", 0], "image": ["1", 0]}},
        "4": {"class_type": "SaveImage", "inputs": {"images": ["3", 0], "filename_prefix": "upscaled"}},
    });
    let out = run_job(port, comfy_root, graph, 900, |_| {})?;
    out.images.into_iter().next().ok_or_else(|| "超分没有产出图片".to_string())
}

/// SeedVR2 on an input-dir file.
pub fn upscale_seedvr2_from_input(
    port: u16,
    comfy_root: &str,
    input_name: &str,
) -> Result<ImageOut, String> {
    let input = PathBuf::from(comfy_root).join("ComfyUI").join("input");
    let (w, h) = png_dims(&input.join(input_name));
    if w == 0 || h == 0 {
        return Err("SeedVR2 目前只支持 PNG 图片".into());
    }
    let tw = ((w / 16).max(1)) * 16;
    let th = ((h / 16).max(1)) * 16;
    let out_w = tw * 4;
    let out_h = th * 4;
    if out_w > 8192 || out_h > 8192 {
        return Err(format!("图太大（{out_w}×{out_h}），SeedVR2 建议先缩到 2000px 以内边长"));
    }
    let graph = json!({
        "1": {"class_type": "LoadImage", "inputs": {"image": input_name}},
        "2": {"class_type": "ResizeImageMaskNode", "inputs": {
            "input": ["1", 0], "resize_type": "scale dimensions",
            "resize_type.width": out_w, "resize_type.height": out_h,
            "resize_type.crop": "center", "scale_method": "lanczos"}},
        "3": {"class_type": "VAELoader", "inputs": {"vae_name": "seedvr2_ema_vae_fp16.safetensors"}},
        "4": {"class_type": "VAEEncodeTiled", "inputs": {
            "pixels": ["2", 0], "vae": ["3", 0],
            "tile_size": 512, "overlap": 128, "temporal_size": 4096, "temporal_overlap": 8}},
        "5": {"class_type": "UNETLoader", "inputs": {
            "unet_name": "seedvr2_3b_int8_convrot.safetensors", "weight_dtype": "default"}},
        "6": {"class_type": "SeedVR2Preprocess", "inputs": {"resized_images": ["2", 0]}},
        "7": {"class_type": "SeedVR2Conditioning", "inputs": {"model": ["5", 0], "vae_conditioning": ["4", 0]}},
        "8": {"class_type": "KSampler", "inputs": {
            "model": ["5", 0], "positive": ["7", 0], "negative": ["7", 1], "latent_image": ["4", 0],
            "seed": 1, "steps": 1, "cfg": 1.0, "sampler_name": "euler", "scheduler": "simple", "denoise": 1.0}},
        "9": {"class_type": "VAEDecodeTiled", "inputs": {
            "samples": ["8", 0], "vae": ["3", 0],
            "tile_size": 512, "overlap": 128, "temporal_size": 4096, "temporal_overlap": 8}},
        "10": {"class_type": "SeedVR2PostProcessing", "inputs": {
            "images": ["9", 0], "original_resized_images": ["2", 0], "color_correction_method": "none"}},
        "11": {"class_type": "SaveImage", "inputs": {"images": ["10", 0], "filename_prefix": "seedvr2"}},
    });
    let out = run_job(port, comfy_root, graph, 1800, |_| {})?;
    out.images.into_iter().next().ok_or_else(|| "SeedVR2 没有产出图片".to_string())
}

/// Dataset folder -> LoRA training graph. Only bf16 checkpoints can be trained.
pub struct TrainOpts<'a> {
    pub base: &'a str,
    pub dataset_folder: &'a str,
    pub name: &'a str,
    pub steps: u32,
    pub rank: u32,
    pub lr: f64,
    pub existing_lora: Option<&'a str>,
    pub bucket: bool,
    pub batch: u32,
    pub grad_accum: u32,
    pub depth: u32,
    pub offload: bool,
    pub dtype: &'a str,
}

pub fn build_train_graph(o: &TrainOpts) -> Result<Value, String> {
    let (unet, clip, clip_type, vae) = match o.base {
        "moss" => (
            "moss_image_sft_bf16.safetensors",
            "moss_te_heretic_full_int8_convrot.safetensors",
            "moss_image",
            "flux2-vae_small.safetensors",
        ),
        _ => (
            "flux3-image-klein-base-bf16.safetensors",
            "flux3-klein-te-heretic-bf16.safetensors",
            "flux3",
            "flux2-vae.safetensors",
        ),
    };
    Ok(json!({
        "1": {"class_type": "UNETLoader", "inputs": {
            "unet_name": unet, "weight_dtype": "default"}},
        "2": {"class_type": "CLIPLoader", "inputs": {
            "clip_name": clip, "type": clip_type, "device": "default"}},
        "3": {"class_type": "VAELoader", "inputs": {"vae_name": vae}},
        "4": {"class_type": "LoadImageTextDataSetFromFolder", "inputs": {"folder": o.dataset_folder}},
        "5": {"class_type": "MakeTrainingDataset", "inputs": {
            "images": ["4", 0], "vae": ["3", 0], "clip": ["2", 0], "texts": ["4", 1]}},
        "6": {"class_type": "TrainLoraNode", "inputs": {
            "model": ["1", 0], "latents": ["5", 0], "positive": ["5", 1],
            "batch_size": o.batch.clamp(1, 4),
            "grad_accumulation_steps": o.grad_accum.clamp(1, 16),
            "steps": o.steps.max(1), "learning_rate": o.lr, "rank": o.rank.max(1),
            "optimizer": "AdamW", "loss_function": "MSE", "seed": 0,
            "training_dtype": o.dtype, "lora_dtype": "bf16", "quantized_backward": false,
            "algorithm": "LoRA", "gradient_checkpointing": true,
            "checkpoint_depth": o.depth.clamp(1, 5),
            "offloading": o.offload,
            "existing_lora": o.existing_lora.unwrap_or("[None]"),
            "bucket_mode": o.bucket, "bypass_mode": false}},
        "7": {"class_type": "SaveLoRA", "inputs": {
            "lora": ["6", 0], "prefix": format!("loras/{}", o.name), "steps": ["6", 2]}},
        "8": {"class_type": "LossGraphNode", "inputs": {
            "loss": ["6", 1], "filename_prefix": format!("{}_loss", o.name)}},
    }))
}

/// bf16 checkpoint each training profile needs, with its size guard (bytes).
pub fn train_checkpoint(base: &str) -> (&'static str, u64) {
    match base {
        "moss" => ("moss_image_sft_bf16.safetensors", 8_500_000_000),
        _ => ("flux3-image-klein-base-bf16.safetensors", 9_000_000_000),
    }
}

/// Inpainting request: both images live in ComfyUI/input.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InpaintReq {
    pub profile: String,
    pub prompt: String,
    #[serde(default)]
    pub negative: String,
    pub source: String,
    pub mask: String,
    pub steps: u32,
    pub cfg: f64,
    #[serde(deserialize_with = "de_seed")]
    pub seed: i64,
    #[serde(default = "one_f")]
    pub denoise: f64,
    #[serde(default)]
    pub feather: u32,
    /// optional second reference image (already copied into input)
    #[serde(default)]
    pub ref_image: Option<String>,
}

/// Masked (local) re-diffusion of one region. White mask = repaint area.
pub fn build_inpaint(req: &InpaintReq) -> Result<Value, String> {
    let p = profile(&req.profile).ok_or_else(|| format!("未知模型配置: {}", req.profile))?;
    if !p.supports_edit {
        return Err("这个模型不支持局部改图".into());
    }
    if req.profile == "mossedit" {
        return Err("MOSS 二编目前只做整图编辑；局部重绘请切到 FLUX.3".into());
    }
    let steps = req.steps.max(1);
    let denoise = if req.denoise > 0.0 { req.denoise } else { 1.0 };
    let mut g: BTreeMap<String, Value> = BTreeMap::new();

    g.insert("1".into(), json!({"class_type": "LoadImage", "inputs": {"image": req.source}}));
    g.insert("2".into(), json!({"class_type": "LoadImage", "inputs": {"image": req.mask}}));
    g.insert("3".into(), json!({"class_type": "ImageToMask", "inputs": {"image": ["2", 0], "channel": "red"}}));
    g.insert("4".into(), json!({"class_type": "VAELoader", "inputs": {"vae_name": p.vae}}));
    g.insert("5".into(), json!({"class_type": "UNETLoader", "inputs": {"unet_name": p.unet, "weight_dtype": "default"}}));
    g.insert("6".into(), json!({"class_type": "CLIPLoader", "inputs": {"clip_name": p.clip, "type": p.clip_type, "device": "default"}}));

    // optional feather: mask -> image -> blur -> mask
    let mask_src: Value = if req.feather > 0 {
        let sigma = (req.feather as f64 / 3.0).max(1.0);
        g.insert("13".into(), json!({"class_type": "MaskToImage", "inputs": {"mask": ["3", 0]}}));
        g.insert(
            "14".into(),
            json!({"class_type": "ImageBlur",
                   "inputs": {"image": ["13", 0], "blur_radius": req.feather, "sigma": sigma}}),
        );
        g.insert("15".into(), json!({"class_type": "ImageToMask", "inputs": {"image": ["14", 0], "channel": "red"}}));
        json!(["15", 0])
    } else {
        json!(["3", 0])
    };
    // FLUX.3（Flux2 VAE，16 通道）不吃 VAEEncodeForInpaint：实测画出来是一块纯色。
    // 它走 InpaintModelConditioning（编码 + 噪声蒙版，任何 VAE 都能用）。
    let flux_inpaint = req.profile.starts_with("flux3");
    if !flux_inpaint {
        g.insert(
            "7".into(),
            json!({"class_type": "VAEEncodeForInpaint",
                   "inputs": {"pixels": ["1", 0], "vae": ["4", 0], "mask": mask_src.clone(), "grow_mask_by": 8}}),
        );
    }

    let (pos, neg): (Value, Value) = if req.profile == "qwen21" {
        let mut te = serde_json::Map::new();
        te.insert("clip".into(), json!(["6", 0]));
        te.insert("prompt".into(), json!(req.prompt));
        te.insert("negative_prompt".into(), json!(req.negative));
        te.insert("resolution".into(), json!(1024));
        te.insert("vae".into(), json!(["4", 0]));
        te.insert("images.image_1".into(), json!(["1", 0]));
        if let Some(refname) = &req.ref_image {
            g.insert("16".into(), json!({"class_type": "LoadImage", "inputs": {"image": refname}}));
            te.insert("images.image_2".into(), json!(["16", 0]));
        }
        g.insert("8".into(), json!({"class_type": "TextEncodeQwenImage21", "inputs": Value::Object(te)}));
        (json!(["8", 0]), json!(["8", 1]))
    } else if flux_inpaint {
        // FLUX.3 局部重绘：底板走 ReferenceMultiLatent 进 conditioning，
        // 再用 InpaintModelConditioning 把「原图 + 蒙版」编成带噪声蒙版的 latent。
        // 这就是「只重画脸、保住底图质感」的那条路。
        g.insert("8".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["6", 0], "text": req.prompt}}));
        g.insert("9".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["6", 0], "text": req.negative}}));
        let (cond_pos, cond_neg): (Value, Value) = match &req.ref_image {
            Some(refname) => {
                g.insert("16".into(), json!({"class_type": "LoadImage", "inputs": {"image": refname}}));
                let mut rm = serde_json::Map::new();
                rm.insert("positive".into(), json!(["8", 0]));
                rm.insert("negative".into(), json!(["9", 0]));
                rm.insert("vae".into(), json!(["4", 0]));
                rm.insert("resolution".into(), json!("keep resolution"));
                rm.insert("images.image0".into(), json!(["16", 0]));
                g.insert("17".into(), json!({"class_type": "ReferenceMultiLatent", "inputs": Value::Object(rm)}));
                (json!(["17", 0]), json!(["17", 1]))
            }
            None => (json!(["8", 0]), json!(["9", 0])),
        };
        g.insert(
            "18".into(),
            json!({"class_type": "InpaintModelConditioning", "inputs": {
                "positive": cond_pos, "negative": cond_neg, "vae": ["4", 0],
                "pixels": ["1", 0], "mask": mask_src.clone(), "noise_mask": true}}),
        );
        (json!(["18", 0]), json!(["18", 1]))
    } else {
        g.insert("8".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["6", 0], "text": req.prompt}}));
        g.insert("9".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"clip": ["6", 0], "text": req.negative}}));
        (json!(["8", 0]), json!(["9", 0]))
    };

    let latent_ref: Value = if flux_inpaint { json!(["18", 2]) } else { json!(["7", 0]) };
    g.insert(
        "10".into(),
        json!({"class_type": "KSampler", "inputs": {
            "model": ["5", 0], "positive": pos, "negative": neg, "latent_image": latent_ref,
            "seed": pick_seed(req.seed), "steps": steps, "cfg": req.cfg,
            "sampler_name": p.sampler, "scheduler": "simple", "denoise": denoise}}),
    );
    g.insert("11".into(), json!({"class_type": "VAEDecode", "inputs": {"samples": ["10", 0], "vae": ["4", 0]}}));
    g.insert("12".into(), json!({"class_type": "SaveImage", "inputs": {"images": ["11", 0], "filename_prefix": "inpaint"}}));

    Ok(Value::Object(g.into_iter().collect()))
}

fn png_dims(path: &std::path::Path) -> (u32, u32) {
    use std::io::Read;
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (0, 0),
    };
    let mut buf = vec![0u8; 64];
    let n = f.read(&mut buf).unwrap_or(0);
    png_size(&buf[..n])
}

/// SeedVR2 (diffusion-based restoration) 4x upscale — much better detail than
/// ESRGAN on faces/text, slower on big images but only 1 sampler step.
pub fn upscale_seedvr2(
    port: u16,
    comfy_root: &str,
    src_path: &str,
) -> Result<ImageOut, String> {
    let input = PathBuf::from(comfy_root).join("ComfyUI").join("input");
    std::fs::create_dir_all(&input).map_err(|e| format!("创建 input 目录失败: {e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = format!("cs_svr_{stamp}.png");
    std::fs::copy(src_path, input.join(&name)).map_err(|e| format!("复制到 input 失败: {e}"))?;

    // SeedVR2 only accepts sizes divisible by 16; snap first, then scale to 4x.
    let (w, h) = png_dims(std::path::Path::new(src_path));
    if w == 0 || h == 0 {
        return Err("SeedVR2 目前只支持 PNG 图片".into());
    }
    let tw = ((w / 16).max(1)) * 16;
    let th = ((h / 16).max(1)) * 16;
    let out_w = tw * 4;
    let out_h = th * 4;
    if out_w > 8192 || out_h > 8192 {
        return Err(format!("图太大（{out_w}×{out_h}），SeedVR2 建议先缩到 2000px 以内边长"));
    }

    let graph = json!({
        "1": {"class_type": "LoadImage", "inputs": {"image": name}},
        "2": {"class_type": "ResizeImageMaskNode", "inputs": {
            "input": ["1", 0], "resize_type": "scale dimensions",
            "resize_type.width": out_w, "resize_type.height": out_h,
            "resize_type.crop": "center", "scale_method": "lanczos"}},
        "3": {"class_type": "VAELoader", "inputs": {"vae_name": "seedvr2_ema_vae_fp16.safetensors"}},
        "4": {"class_type": "VAEEncodeTiled", "inputs": {
            "pixels": ["2", 0], "vae": ["3", 0],
            "tile_size": 512, "overlap": 128, "temporal_size": 4096, "temporal_overlap": 8}},
        "5": {"class_type": "UNETLoader", "inputs": {
            "unet_name": "seedvr2_3b_int8_convrot.safetensors", "weight_dtype": "default"}},
        "6": {"class_type": "SeedVR2Preprocess", "inputs": {"resized_images": ["2", 0]}},
        "7": {"class_type": "SeedVR2Conditioning", "inputs": {"model": ["5", 0], "vae_conditioning": ["4", 0]}},
        "8": {"class_type": "KSampler", "inputs": {
            "model": ["5", 0], "positive": ["7", 0], "negative": ["7", 1], "latent_image": ["4", 0],
            "seed": 1, "steps": 1, "cfg": 1.0, "sampler_name": "euler", "scheduler": "simple", "denoise": 1.0}},
        "9": {"class_type": "VAEDecodeTiled", "inputs": {
            "samples": ["8", 0], "vae": ["3", 0],
            "tile_size": 512, "overlap": 128, "temporal_size": 4096, "temporal_overlap": 8}},
        "10": {"class_type": "SeedVR2PostProcessing", "inputs": {
            "images": ["9", 0], "original_resized_images": ["2", 0], "color_correction_method": "none"}},
        "11": {"class_type": "SaveImage", "inputs": {"images": ["10", 0], "filename_prefix": "seedvr2"}},
    });
    let out = run_job(port, comfy_root, graph, 1800, |_| {})?;
    out.images.into_iter().next().ok_or_else(|| "SeedVR2 没有产出图片".to_string())
}

/// 4x model upscale of one already-generated image.
pub fn upscale_image(    port: u16,
    comfy_root: &str,
    src_path: &str,
    up_model: &str,
) -> Result<ImageOut, String> {
    let input = PathBuf::from(comfy_root).join("ComfyUI").join("input");
    std::fs::create_dir_all(&input).map_err(|e| format!("创建 input 目录失败: {e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = format!("cs_up_{stamp}.png");
    std::fs::copy(src_path, input.join(&name)).map_err(|e| format!("复制到 input 失败: {e}"))?;

    let graph = json!({
        "1": {"class_type": "LoadImage", "inputs": {"image": name}},
        "2": {"class_type": "UpscaleModelLoader", "inputs": {"model_name": up_model}},
        "3": {"class_type": "ImageUpscaleWithModel",
              "inputs": {"upscale_model": ["2", 0], "image": ["1", 0]}},
        "4": {"class_type": "SaveImage", "inputs": {"images": ["3", 0], "filename_prefix": "upscaled"}},
    });
    let out = run_job(port, comfy_root, graph, 900, |_| {})?;
    out.images.into_iter().next().ok_or_else(|| "超分没有产出图片".to_string())
}

/// Submit a graph and wait for it to finish, returning the `outputs` object.
/// Used by text-only jobs (prompt enhancement) and custom workflows.
pub fn run_raw(port: u16, graph: Value, timeout_secs: u64) -> Result<Value, String> {
    let start = Instant::now();
    let submit = client(30)
        .post(format!("{}/prompt", base(port)))
        .json(&json!({"prompt": graph, "client_id": "comfyshell"}))
        .send()
        .map_err(|e| format!("提交任务失败: {e}"))?;
    if !submit.status().is_success() {
        let txt = submit.text().unwrap_or_default();
        return Err(format!(
            "ComfyUI 拒绝任务: {}",
            txt.chars().take(600).collect::<String>()
        ));
    }
    let body: Value = submit.json().map_err(|e| format!("响应解析失败: {e}"))?;
    let prompt_id = body
        .get("prompt_id")
        .and_then(|v| v.as_str())
        .ok_or("ComfyUI 未返回 prompt_id")?
        .to_string();

    let hist_client = client(10);
    loop {
        std::thread::sleep(Duration::from_millis(600));
        if start.elapsed().as_secs() > timeout_secs {
            return Err(format!("超时（{timeout_secs}s）"));
        }
        let resp = match hist_client
            .get(format!("{}/history/{}", base(port), prompt_id))
            .send()
        {
            Ok(r) => r,
            Err(_) => continue,
        };
        let hist: Value = match resp.json() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(entry) = hist.get(&prompt_id) else { continue };
        let status = entry.get("status").cloned().unwrap_or(Value::Null);
        let completed = status.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);
        let status_str = status.get("status_str").and_then(|v| v.as_str()).unwrap_or("");
        if status_str == "error" {
            let msg = status
                .get("messages")
                .and_then(|m| m.as_array())
                .and_then(|arr| {
                    arr.iter()
                        .find(|m| m.get(0).and_then(|x| x.as_str()) == Some("execution_error"))
                })
                .and_then(|m| m.get(1))
                .and_then(|m| m.get("exception_message"))
                .and_then(|m| m.as_str())
                .unwrap_or("执行失败");
            return Err(format!("ComfyUI 执行失败: {msg}"));
        }
        if completed {
            return Ok(entry.get("outputs").cloned().unwrap_or(Value::Null));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(json_text: &str) -> GenReq {
        serde_json::from_str(json_text).expect("GenReq 反序列化")
    }

    fn base(profile: &str, refs: &[&str]) -> GenReq {
        req(&format!(
            r#"{{"profile":"{profile}","mode":"edit","prompt":"换成皮夹克","width":1024,"height":1365,
                 "steps":30,"cfg":4.0,"seed":1,"refImages":{}}}"#,
            serde_json::to_string(refs).unwrap()
        ))
    }

    #[test]
    fn moss_edit_graph_uses_official_nodes() {
        let g = build_graph(&base("mossedit", &["ref.png"]), false).unwrap();
        // 文本编码走官方的 TextEncodeMOSSImageEdit，且 edit_format 必须是这个 LoRA 训练时的布局
        for nid in ["5", "6"] {
            let n = &g[nid];
            assert_eq!(n["class_type"], "TextEncodeMOSSImageEdit");
            assert_eq!(n["inputs"]["edit_format"], "Moss-Image-Edit");
            assert_eq!(n["inputs"]["images.image_0"], json!(["20", 0]));
            assert_eq!(n["inputs"]["vae"], json!(["4", 0]));
        }
        // 模型链：UNET -> 编辑 LoRA -> SLG(DiT Simple)
        assert_eq!(g["31"]["class_type"], "LoraLoaderModelOnly");
        assert_eq!(g["31"]["inputs"]["lora_name"], MOSS_EDIT_LORA);
        assert_eq!(g["32"]["class_type"], "SkipLayerGuidanceDiTSimple");
        assert_eq!(g["8"]["inputs"]["model"], json!(["32", 0]));
        // 采样器拿到的是两个 TextEncodeMOSSImageEdit，不是 FLUX 那条 ReferenceMultiLatent
        assert_eq!(g["8"]["inputs"]["positive"], json!(["5", 0]));
        assert_eq!(g["8"]["inputs"]["negative"], json!(["6", 0]));
        assert!(g.get("7").is_some());
        // 1365 不是 16 的倍数：MOSS 会拒，必须被截到 1360
        assert_eq!(g["7"]["inputs"]["height"], 1360);
        assert_eq!(g["7"]["inputs"]["width"], 1024);
    }


    #[test]
    fn moss_edit_without_refs_falls_back_to_t2i() {
        let g = build_graph(&base("mossedit", &[]), false).unwrap();
        assert_eq!(g["5"]["class_type"], "CLIPTextEncode");
        assert_eq!(g["6"]["class_type"], "CLIPTextEncode");
        assert!(g.get("31").is_none(), "没参考图就不该挂编辑 LoRA");
        assert_eq!(g["7"]["inputs"]["height"], 1360);
    }

    #[test]
    fn kromoss_always_applies_its_8_step_lora() {
        let mut r = base("kromoss", &[]);
        r.mode = "t2i".into();
        let g = build_graph(&r, false).unwrap();
        assert_eq!(g["30"]["class_type"], "LoraLoaderModelOnly");
        assert_eq!(g["30"]["inputs"]["lora_name"], KROMOSS_TURBO_LORA);
        assert_eq!(g["8"]["inputs"]["model"], json!(["30", 0]), "KSampler 必须用到挂了 Turbo 的模型");
        // 没挂参考图：走普通文生图两路编码
        assert_eq!(g["5"]["class_type"], "CLIPTextEncode");
    }

    #[test]
    fn flux_inpaint_with_plate_uses_reference_latents() {
        let base = |ref_image: Option<&str>| InpaintReq {
            profile: "flux3".into(),
            prompt: "同一个人".into(),
            negative: String::new(),
            source: "src.png".into(),
            mask: "mask.png".into(),
            steps: 16,
            cfg: 1.0,
            seed: 1,
            denoise: 0.55,
            feather: 12,
            ref_image: ref_image.map(|s| s.to_string()),
        };
        let g = build_inpaint(&base(Some("plate.png"))).unwrap();
        assert_eq!(g["17"]["class_type"], "ReferenceMultiLatent");
        assert_eq!(g["17"]["inputs"]["images.image0"], json!(["16", 0]));
        // FLUX 的 Flux2 VAE 不吃 VAEEncodeForInpaint（实测出纯色块），必须走 InpaintModelConditioning
        assert!(g.get("7").is_none(), "FLUX 不该再用 VAEEncodeForInpaint");
        assert_eq!(g["18"]["class_type"], "InpaintModelConditioning");
        assert_eq!(g["10"]["inputs"]["positive"], json!(["18", 0]));
        assert_eq!(g["10"]["inputs"]["latent_image"], json!(["18", 2]));
        assert_eq!(g["10"]["inputs"]["denoise"], 0.55, "重绘强度要透传");

        // 不带底板：仍然走 InpaintModelConditioning，只是没有 ReferenceMultiLatent
        let g2 = build_inpaint(&base(None)).unwrap();
        assert!(g2.get("17").is_none());
        assert_eq!(g2["18"]["inputs"]["positive"], json!(["8", 0]));
        assert_eq!(g2["10"]["inputs"]["latent_image"], json!(["18", 2]));

        // MOSS 二编不支持局部重绘：要给明确错误，而不是悄悄画坏
        let mut r = base(None);
        r.profile = "mossedit".into();
        assert!(build_inpaint(&r).is_err());
    }

    #[test]
    fn flux_edit_keeps_its_own_path() {
        let g = build_graph(&base("flux3", &["ref.png"]), false).unwrap();
        assert_eq!(g["7"]["class_type"], "ReferenceMultiLatent");
        assert!(g.get("31").is_none());
    }
}




