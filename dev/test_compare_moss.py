import json, time, urllib.request, os
from PIL import Image, ImageDraw

API = "http://127.0.0.1:8188"
OUT = r"D:\deepseek-files\ai-tools\ComfyUI_windows_portable\ComfyUI\output"
PROJ = r"D:\deepseek-files\ai-tools\comfy-shell"

PROMPT = ("portrait of a woman standing in a rainy street at night, cinematic photo, "
          "natural skin texture, detailed clothing, film grain, shallow depth of field")
NEG = "face lighting, studio light, bright rim light, nsfw, worst quality, bad anatomy, watermark"
SEED, STEPS, CFG = 42, 50, 3.0


def run(tag, unet, lora=None, strength=0.7):
    wf = {
        "1": {"class_type": "UNETLoader", "inputs": {"unet_name": unet, "weight_dtype": "default"}},
        "2": {"class_type": "CLIPLoader", "inputs": {"clip_name": "moss_te_heretic_full_int8_convrot.safetensors", "type": "moss_image", "device": "default"}},
        "3": {"class_type": "VAELoader", "inputs": {"vae_name": "flux2-vae_small.safetensors"}},
        "5": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["2", 0], "text": PROMPT}},
        "6": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["2", 0], "text": NEG}},
        "7": {"class_type": "EmptyLatentImage", "inputs": {"width": 512, "height": 512, "batch_size": 1}},
        "8": {"class_type": "KSampler", "inputs": {"model": ["1", 0], "positive": ["5", 0], "negative": ["6", 0],
                                                   "latent_image": ["7", 0], "seed": SEED, "steps": STEPS, "cfg": CFG,
                                                   "sampler_name": "euler", "scheduler": "simple", "denoise": 1.0}},
        "9": {"class_type": "VAEDecode", "inputs": {"samples": ["8", 0], "vae": ["3", 0]}},
        "10": {"class_type": "SaveImage", "inputs": {"images": ["9", 0], "filename_prefix": tag}},
    }
    # SLG skip 22, same as the shell's default
    wf["11"] = {"class_type": "SkipLayerGuidanceDiT", "inputs": {
        "model": ["1", 0], "double_layers": "", "single_layers": "22",
        "scale": 3.0, "start_percent": 0.01, "end_percent": 1.0, "rescaling_scale": 0.0}}
    wf["8"]["inputs"]["model"] = ["11", 0]
    if lora:
        wf["12"] = {"class_type": "LoraLoaderModelOnly", "inputs": {
            "model": ["11", 0], "lora_name": lora, "strength_model": strength}}
        wf["8"]["inputs"]["model"] = ["12", 0]
    req = urllib.request.Request(API + "/prompt", data=json.dumps({"prompt": wf}).encode(),
                                 headers={"Content-Type": "application/json"})
    try:
        r = json.load(urllib.request.urlopen(req, timeout=30))
    except Exception as e:
        print(tag, "REJECTED", (e.read().decode() if hasattr(e, "read") else str(e))[:300], flush=True)
        return None
    pid = r["prompt_id"]; t0 = time.time()
    while time.time() - t0 < 900:
        time.sleep(3)
        h = json.load(urllib.request.urlopen(f"{API}/history/{pid}", timeout=20))
        if pid in h:
            st = h[pid]["status"]; msg = ""
            for m in st.get("messages", []):
                if m[0] == "execution_error": msg = (m[1].get("exception_message") or "")[:200]
            imgs = [im["filename"] for o in h[pid].get("outputs", {}).values() for im in o.get("images", [])]
            print(tag, st.get("status_str"), int(time.time() - t0), "s", msg, imgs, flush=True)
            return os.path.join(OUT, imgs[0]) if imgs else None
    return None


SFT = "moss_image_sft_int8_convrot.safetensors"
IMG = "MOSS-Imagine-INT8-ConvRot.safetensors"
BEAUTY = "MOSS-Image-Beauty-LoRA.safetensors"
ULTRA = "ultra_real_moss_v2.safetensors"

results = [
    ("1 MOSS SFT + Beauty 0.7", run("cmp_sft_beauty", SFT, BEAUTY, 0.7)),
    ("2 MOSS SFT + UltraReal 0.7", run("cmp_sft_ultra", SFT, ULTRA, 0.7)),
    ("3 MOSS-Imagine", run("cmp_img_plain", IMG)),
    ("4 MOSS-Imagine + Beauty 0.7", run("cmp_img_beauty", IMG, BEAUTY, 0.7)),
]

cells = []
for label, path in results:
    if path and os.path.exists(path):
        im = Image.open(path).convert("RGB").resize((512, 512))
        cells.append((label, im))
W = 512 * 2 + 30
H = 512 * 2 + 100
canvas = Image.new("RGB", (W, H), (16, 18, 24))
d = ImageDraw.Draw(canvas)
for i, (label, im) in enumerate(cells):
    x = 10 + (i % 2) * 522
    y = 30 + (i // 2) * 540
    canvas.paste(im, (x, y))
    d.text((x + 8, y + 8), label, fill=(255, 255, 255))
canvas.save(os.path.join(PROJ, "shot_moss_compare.png"))
print("saved", len(cells), "cells", flush=True)
