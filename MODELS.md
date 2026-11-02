# 模型清单（显影不自带任何模型）

显影只内置**工作流**（构图、采样、参考图链路写在 `src-tauri/src/api.rs` 的 Profile 里），
**不打包、不自动下载**任何模型权重。下面每份模型都由你自行获取，并遵循它自己的许可。

放置位置：`<ComfyUI 便携版>\ComfyUI\models\<kind>\<文件名>`（kind 见下表）。

## 内置档位需要什么

| 档位 | 必需文件 | kind |
|---|---|---|
| Qwen-Image 2.1 | `qwen_image_2.1_int8_convrot.safetensors` · `qwen3vl_8b_int8_convrot_heretic.safetensors` · `qwen_image_2.1_vae_bf16.safetensors` | diffusion_models / text_encoders / vae |
| FLUX.3 Klein（蒸馏） | `flux3-image-klein-int8-convrot.safetensors` · `flux3-klein-te-heretic-bf16.safetensors` · `flux2-vae.safetensors` | diffusion_models / text_encoders / vae |
| FLUX.3 Klein Base | `flux3-image-klein-base-int8-convrot.safetensors` · 同上 TE/VAE | 同上 |
| MOSS-Image SFT | `moss_image_sft_int8_convrot.safetensors` · `moss_image_te_*.safetensors` · `moss_image_vae.safetensors` | 同上 |
| MOSS-Image Base / Imagine / 编辑 | 各自 int8/bf16 权重 + 编辑 LoRA `moss_image_edit_lora_rank_64_bf16.safetensors` | + loras |
| Kromoss 8 步 | `kromoss_int8_convrot.safetensors` + `kromoss_8steps_dmd_lora.safetensors` | diffusion_models + loras |

> 注意：`*_heretic_*` 这类文本编码器是**本机二次处理过的**（去审查），
> 没有公开下载地址；换用官方 TE 也能跑，但扩写/提示词理解口径会变。

## 来源与许可（按需自取）

| 模型家族 | 常见来源 | 许可 |
|---|---|---|
| FLUX.3 Klein（BFL） | `black-forest-labs/flux-3-image-klein-comfy`（Hugging Face） | FLUX Kommunity License（非 Apache；商用前读原文） |
| FLUX.2 VAE / 组件 | Comfy-Org 的 FLUX 系列仓库 | 随原模型 |
| Qwen-Image 2.1 | Comfy-Org / Qwen 官方仓库 | 随原模型 |
| MOSS-Image | Comfy-Org / MOSS 作者仓库 | 随原模型 |
| 社区 LoRA | 各自 Hugging Face / Civitai 页面 | 各自许可（常见为非商用） |

## 用显影下载（可选）

设置 → 工作流 会列出每个档位缺的文件，点「下载」即可（**hf-mirror 优先，失败回退 HuggingFace**），
下到 `<ComfyUI>/models/<kind>/`。清单只覆盖内置档位需要的文件，不含你自己找的 LoRA/工作流。

> 本机的 `*_heretic_*` 文本编码器没有公开源，清单用同架构的公开版代替并标注；
> 下载后按 profile 期望的文件名落盘，所以档位能直接跑。

## 为什么不做「一键下模型」

- 体积：单档 5–20GB，装错盘/装一半失败比手动更糟；
- 许可：模型不是 Apache-2.0，替用户下载等于替用户接受条款；
- 可替代：同一个档位可以换 TE/VAE，不同量化版本的显存占用完全不同。

显影的「设置 → 工作流」会告诉你**每个档位缺哪个文件**，缺什么补什么。
`file_download` 命令已经具备多线程下载能力，将来若接入「模型清单 + 直链」，
也只是把用户点的那一个文件下到对应 `models/<kind>/`，不会静默下载。
