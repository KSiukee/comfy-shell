import json, time, urllib.request

API = "http://127.0.0.1:8188"
ROOT = r"D:\deepseek-files\ai-tools\ComfyUI_windows_portable"


def run(tag, slg):
    model_src = ["1", 0]
    nodes = {
        "1": {"class_type": "UNETLoader", "inputs": {"unet_name": "moss_image_sft_int8_convrot.safetensors", "weight_dtype": "default"}},
        "2": {"class_type": "CLIPLoader", "inputs": {"clip_name": "moss_te_heretic_full_int8_convrot.safetensors", "type": "moss_image", "device": "default"}},
        "3": {"class_type": "VAELoader", "inputs": {"vae_name": "flux2-vae_small.safetensors"}},
        "5": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["2", 0], "text": "portrait of a woman standing in a rainy street at night, no street lights, dark"}},
        "6": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["2", 0], "text": "face lighting, studio light, bright rim light, nsfw, worst quality, bad anatomy"}},
        "7": {"class_type": "EmptyLatentImage", "inputs": {"width": 512, "height": 512, "batch_size": 1}},
        "9": {"class_type": "VAEDecode", "inputs": {"samples": ["8", 0], "vae": ["3", 0]}},
        "10": {"class_type": "SaveImage", "inputs": {"images": ["9", 0], "filename_prefix": tag}},
    }
    if slg:
        nodes["11"] = {"class_type": "SkipLayerGuidanceDiT", "inputs": {
            "model": ["1", 0], "double_layers": "", "single_layers": "22",
            "scale": 3.0, "start_percent": 0.01, "end_percent": 1.0, "rescaling_scale": 0.0}}
        model_src = ["11", 0]
    nodes["8"] = {"class_type": "KSampler", "inputs": {
        "model": model_src, "positive": ["5", 0], "negative": ["6", 0], "latent_image": ["7", 0],
        "seed": 42, "steps": 50, "cfg": 3.0, "sampler_name": "euler", "scheduler": "simple", "denoise": 1.0}}
    req = urllib.request.Request(API + "/prompt", data=json.dumps({"prompt": nodes}).encode(),
                                 headers={"Content-Type": "application/json"})
    try:
        r = json.load(urllib.request.urlopen(req, timeout=30))
    except Exception as e:
        print(tag, "REJECTED", (e.read().decode() if hasattr(e, "read") else str(e))[:400])
        return
    pid = r["prompt_id"]
    t0 = time.time()
    while time.time() - t0 < 900:
        time.sleep(3)
        h = json.load(urllib.request.urlopen(f"{API}/history/{pid}", timeout=20))
        if pid in h:
            st = h[pid]["status"]
            msg = ""
            for m in st.get("messages", []):
                if m[0] == "execution_error":
                    msg = (m[1].get("exception_message") or "")[:250]
            imgs = [im["filename"] for o in h[pid].get("outputs", {}).values() for im in o.get("images", [])]
            print(tag, st.get("status_str"), int(time.time() - t0), "s", msg, imgs)
            return
    print(tag, "timeout")


run("moss_slg_off", False)
run("moss_slg_on", True)
