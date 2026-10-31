import json, time, urllib.request

API = "http://127.0.0.1:8188"


def probe(tag, unet, clip, ctype, vae, steps=2, depth=3, offload=False, dtype="bf16", qb=False, folder="moss_ds_384"):
    wf = {
        "1": {"class_type": "UNETLoader", "inputs": {"unet_name": unet, "weight_dtype": "default"}},
        "2": {"class_type": "CLIPLoader", "inputs": {"clip_name": clip, "type": ctype, "device": "default"}},
        "3": {"class_type": "VAELoader", "inputs": {"vae_name": vae}},
        "4": {"class_type": "LoadImageTextDataSetFromFolder", "inputs": {"folder": folder}},
        "5": {"class_type": "MakeTrainingDataset", "inputs": {"images": ["4", 0], "vae": ["3", 0], "clip": ["2", 0], "texts": ["4", 1]}},
        "6": {"class_type": "TrainLoraNode", "inputs": {
            "model": ["1", 0], "latents": ["5", 0], "positive": ["5", 1],
            "batch_size": 1, "grad_accumulation_steps": 1,
            "steps": steps, "learning_rate": 0.0002, "rank": 8,
            "optimizer": "AdamW", "loss_function": "MSE", "seed": 0,
            "training_dtype": dtype, "lora_dtype": "bf16", "quantized_backward": qb,
            "algorithm": "LoRA", "gradient_checkpointing": True, "checkpoint_depth": depth,
            "offloading": offload, "existing_lora": "[None]", "bucket_mode": False, "bypass_mode": False}},
        "7": {"class_type": "SaveLoRA", "inputs": {"lora": ["6", 0], "prefix": "loras/" + tag, "steps": ["6", 2]}},
    }
    req = urllib.request.Request(API + "/prompt", data=json.dumps({"prompt": wf}).encode(),
                                 headers={"Content-Type": "application/json"})
    try:
        r = json.load(urllib.request.urlopen(req, timeout=30))
    except Exception as e:
        print(tag, "REJECTED", (e.read().decode() if hasattr(e, "read") else str(e))[:300], flush=True)
        return
    pid = r["prompt_id"]
    t0 = time.time()
    while time.time() - t0 < 600:
        time.sleep(4)
        try:
            h = json.load(urllib.request.urlopen(f"{API}/history/{pid}", timeout=20))
        except Exception:
            print(tag, "server died at", int(time.time() - t0), "s", flush=True)
            return
        if pid in h:
            st = h[pid]["status"]
            msg = ""
            for m in st.get("messages", []):
                if m[0] == "execution_error":
                    msg = (m[1].get("exception_message") or "")[:200]
            print(tag, st.get("status_str"), int(time.time() - t0), "s", msg, flush=True)
            return
    print(tag, "timeout", flush=True)


MOSS = ("moss_image_sft_bf16.safetensors", "moss_te_heretic_full_int8_convrot.safetensors", "moss_image",
        "flux2-vae_small.safetensors")

probe("moss384_nooff", *MOSS, offload=False, depth=3)
probe("moss384_d1", *MOSS, offload=False, depth=1)
