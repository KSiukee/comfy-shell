import re
proj = r"D:\deepseek-files\ai-tools\comfy-shell"

p = proj + r"\src-tauri\src\api.rs"
s = open(p, encoding="utf-8").read()

block = '''        "mossimagine" => Profile {
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
'''
if '"mossimagine"' not in s:
    anchor = '        "mossbase" => Profile {'
    s = s.replace(anchor, block + anchor, 1)

s = s.replace('for name in ["qwen21", "flux3", "flux3base", "moss", "mossbase"] {',
              'for name in ["qwen21", "flux3", "flux3base", "moss", "mossbase", "mossimagine"] {', 1)
open(p, "w", encoding="utf-8", newline="\n").write(s)
print("profile ok")

p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")
j = j.replace('{ test: /moss/i, models: ["moss", "mossbase"]', '{ test: /moss/i, models: ["moss", "mossbase", "mossimagine"]', 1)
j = j.replace('{ test: /MOSS-Image-NSFW/i, models: ["moss", "mossbase"]', '{ test: /MOSS-Image-NSFW/i, models: ["moss", "mossbase", "mossimagine"]', 1)
open(p, "w", encoding="utf-8", newline="\n").write(j)
print("rules ok")
