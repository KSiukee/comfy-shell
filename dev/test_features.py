"""Feature test for ComfyShell: prompt enhancement, custom workflow import, history."""
import asyncio
import base64
import json
import time
import urllib.request

import websockets

CUSTOM_GRAPH = {
    "1": {"class_type": "UNETLoader", "inputs": {"unet_name": "flux3-image-klein-int8-convrot.safetensors", "weight_dtype": "default"}},
    "2": {"class_type": "CLIPLoader", "inputs": {"clip_name": "flux3-klein-te-heretic-bf16.safetensors", "type": "flux3", "device": "default"}},
    "3": {"class_type": "VAELoader", "inputs": {"vae_name": "flux2-vae.safetensors"}},
    "4": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["2", 0], "text": "{{prompt}}"}},
    "5": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["2", 0], "text": ""}},
    "6": {"class_type": "EmptyFlux2LatentImage", "inputs": {"width": 1024, "height": 1024, "batch_size": 1}},
    "7": {"class_type": "KSampler", "inputs": {"model": ["1", 0], "positive": ["4", 0], "negative": ["5", 0],
                                                "latent_image": ["6", 0], "seed": "{{seed}}", "steps": 16,
                                                "cfg": 1.0, "sampler_name": "er_sde", "scheduler": "simple", "denoise": 1.0}},
    "8": {"class_type": "VAEDecode", "inputs": {"samples": ["7", 0], "vae": ["3", 0]}},
    "9": {"class_type": "SaveImage", "inputs": {"images": ["8", 0], "filename_prefix": "custom_flux3"}},
}


def page_ws():
    data = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json", timeout=5).read())
    return [p for p in data if p.get("type") == "page"][0]["webSocketDebuggerUrl"]


class CDP:
    def __init__(self, ws):
        self.ws = ws
        self.id = 0

    async def call(self, method, params=None):
        self.id += 1
        await self.ws.send(json.dumps({"id": self.id, "method": method, "params": params or {}}))
        while True:
            msg = json.loads(await self.ws.recv())
            if msg.get("id") == self.id:
                return msg

    async def eval(self, expr, timeout=120):
        r = await asyncio.wait_for(
            self.call("Runtime.evaluate", {"expression": expr, "awaitPromise": True, "returnByValue": True}),
            timeout,
        )
        res = r.get("result", {})
        if "exceptionDetails" in res:
            raise RuntimeError(str(res["exceptionDetails"])[:600])
        return res.get("result", {}).get("value")

    async def shot(self, path):
        r = await self.call("Page.captureScreenshot", {"format": "png"})
        open(path, "wb").write(base64.b64decode(r["result"]["data"]))


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        await c.call("Runtime.enable")

        # 0. state
        st = await c.eval("""(() => ({
            cards: document.querySelectorAll('.card').length,
            models: document.querySelectorAll('#model option').length,
            status: document.getElementById('statusText').textContent
        }))()""")
        print("boot:", st, flush=True)
        if not st["status"].startswith("ComfyUI"):
            print("waiting for server…", flush=True)
            for _ in range(60):
                await asyncio.sleep(1)
                s = await c.eval("document.getElementById('statusText').textContent")
                if s.startswith("ComfyUI"):
                    break

        # 1. prompt enhancement (Qwen 2.1 t2i PE)
        await c.eval("""(() => {
            const el = document.getElementById('prompt');
            el.value = '一只柴犬在海边冲浪，黄昏';
            el.dispatchEvent(new Event('input', {bubbles:true}));
            document.getElementById('btnEnhance').click();
        })()""")
        enh = None
        for _ in range(90):
            await asyncio.sleep(2)
            enh = await c.eval("""(() => ({
                busy: document.getElementById('btnEnhance').textContent,
                open: !document.getElementById('enhanced').hidden,
                len: document.getElementById('enhancedText').value.length,
                head: document.getElementById('enhancedText').value.slice(0, 120),
                toast: document.getElementById('toast').hidden ? '' : document.getElementById('toast').textContent
            }))()""")
            if enh["open"] or (enh["busy"] == "✨ 扩写" and enh["toast"]):
                break
        print("enhance:", enh, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_enhance.png")

        # 2. adopt it and generate
        await c.eval("document.getElementById('enhUse').click()")
        await c.eval("document.getElementById('btnGenerate').click()")
        for _ in range(90):
            await asyncio.sleep(2)
            g = await c.eval("({gen: document.getElementById('btnGenerate').disabled, cards: document.querySelectorAll('.card').length, toast: document.getElementById('toast').hidden?'':document.getElementById('toast').textContent})")
            if not g["gen"]:
                break
        print("generate after enhance:", g, flush=True)

        # 3. import a custom workflow and run it
        graph = json.dumps(CUSTOM_GRAPH)
        saved = await c.eval(f"""window.__TAURI__.core.invoke('custom_save', {{name: 'flux3_t2i_自定义', jsonText: {json.dumps(graph)}}})""")
        print("custom_save ->", saved, flush=True)
        customs = await c.eval("window.__TAURI__.core.invoke('custom_list')")
        print("custom_list ->", customs, flush=True)
        # reload so the freshly imported workflow shows up in the model dropdown
        await c.eval("location.reload(); 'reloading'")
        await asyncio.sleep(6)
        await c.eval("""(async () => {
            const s = document.getElementById('model');
            const opt = [...s.options].find(o => o.value.startsWith('custom:'));
            s.value = opt.value;
            s.dispatchEvent(new Event('change', {bubbles:true}));
            document.getElementById('prompt').value = 'a quiet forest shrine at dawn, mist, cinematic';
            document.getElementById('btnGenerate').click();
        })()""")
        for _ in range(90):
            await asyncio.sleep(2)
            g2 = await c.eval("({gen: document.getElementById('btnGenerate').disabled, cards: document.querySelectorAll('.card').length, toast: document.getElementById('toast').hidden?'':document.getElementById('toast').textContent})")
            if not g2["gen"]:
                break
        print("custom generate:", g2, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_features.png")

        hist = await c.eval("window.__TAURI__.core.invoke('history_list')")
        print("history entries:", len(hist), "| newest:", hist[0]["label"], hist[0]["mode"], flush=True)


asyncio.run(main())

