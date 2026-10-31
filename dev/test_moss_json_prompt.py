import json, time, urllib.request, os
from PIL import Image, ImageDraw

API = "http://127.0.0.1:8188"
OUT = r"D:\deepseek-files\ai-tools\ComfyUI_windows_portable\ComfyUI\output"
PROJ = r"D:\deepseek-files\ai-tools\comfy-shell"

NAT = ("a young woman sitting on a wooden windowsill in a sunlit cafe, wearing a cream knit sweater, "
       "holding a ceramic coffee cup with both hands, warm afternoon sunlight from the left, soft shadows, "
       "photorealistic photo, 35mm lens, shallow depth of field, green plants and wooden shelves in the background, "
       "cozy quiet atmosphere, detailed skin texture, film grain")
JSN = json.dumps({
    "subject": "young woman",
    "action": "sitting on a wooden windowsill",
    "prop": "ceramic coffee cup held with both hands",
    "clothing": "cream knit sweater",
    "setting": "sunlit cafe, wooden shelves, green plants",
    "lighting": "warm afternoon sunlight from the left, soft shadows",
    "style": "photorealistic, 35mm photo, film grain",
    "camera": "shallow depth of field",
    "details": "detailed skin texture",
    "mood": "cozy, quiet",
}, ensure_ascii=False)
NEG = "nsfw, worst quality, bad anatomy, watermark, jpeg artifact"


def run(tag, text, seed):
    wf = {
        "1": {"class_type": "UNETLoader", "inputs": {"unet_name": "moss_image_sft_int8_convrot.safetensors", "weight_dtype": "default"}},
        "2": {"class_type": "CLIPLoader", "inputs": {"clip_name": "moss_te_heretic_full_int8_convrot.safetensors", "type": "moss_image", "device": "default"}},
        "3": {"class_type": "VAELoader", "inputs": {"vae_name": "flux2-vae_small.safetensors"}},
        "5": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["2", 0], "text": text}},
        "6": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["2", 0], "text": NEG}},
        "7": {"class_type": "EmptyLatentImage", "inputs": {"width": 512, "height": 512, "batch_size": 1}},
        "8": {"class_type": "KSampler", "inputs": {"model": ["1", 0], "positive": ["5", 0], "negative": ["6", 0],
                                                   "latent_image": ["7", 0], "seed": seed, "steps": 50, "cfg": 3.0,
                                                   "sampler_name": "euler", "scheduler": "simple", "denoise": 1.0}},
        "9": {"class_type": "VAEDecode", "inputs": {"samples": ["8", 0], "vae": ["3", 0]}},
        "10": {"class_type": "SaveImage", "inputs": {"images": ["9", 0], "filename_prefix": tag}},
        "11": {"class_type": "SkipLayerGuidanceDiT", "inputs": {"model": ["1", 0], "double_layers": "", "single_layers": "22",
                                                                "scale": 3.0, "start_percent": 0.01, "end_percent": 1.0,
                                                                "rescaling_scale": 0.0}},
    }
    wf["8"]["inputs"]["model"] = ["11", 0]
    req = urllib.request.Request(API + "/prompt", data=json.dumps({"prompt": wf}).encode(), headers={"Content-Type": "application/json"})
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


cells = []
for label, text in (("自然语言", NAT), ("JSON", JSN)):
    for seed in (1234, 5678):
        p = run(f"mossfmt_{label}_{seed}", text, seed)
        cells.append((f"{label} · seed {seed}", p))

canvas = Image.new("RGB", (512 * 2 + 30, 512 * 2 + 90), (16, 18, 24))
d = ImageDraw.Draw(canvas)
for i, (label, path) in enumerate(cells):
    if not path or not os.path.exists(path):
        continue
    im = Image.open(path).convert("RGB").resize((512, 512))
    x = 10 + (i % 2) * 522
    y = 30 + (i // 2) * 530
    canvas.paste(im, (x, y))
    d.text((x + 8, y + 8), label, fill=(255, 255, 255))
canvas.save(os.path.join(PROJ, "shot_moss_json_compare.png"))
print("saved", flush=True)
