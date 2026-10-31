"""Qwen 2.1 + Anything2Real edit test."""
import asyncio, json, os, base64, urllib.request
import websockets

ROOT = r"D:\deepseek-files\ai-tools\ComfyUI_windows_portable"
OUT = r"D:\deepseek-files\ai-tools\comfy-shell"


def page_ws():
    d = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json", timeout=5).read())
    return [p for p in d if p.get("type") == "page"][0]["webSocketDebuggerUrl"]


class CDP:
    def __init__(self, ws):
        self.ws = ws
        self.id = 0

    async def call(self, method, params=None):
        self.id += 1
        await self.ws.send(json.dumps({"id": self.id, "method": method, "params": params or {}}))
        while True:
            m = json.loads(await self.ws.recv())
            if m.get("id") == self.id:
                return m

    async def eval(self, expr, timeout=1200):
        r = await asyncio.wait_for(self.call("Runtime.evaluate", {"expression": expr, "awaitPromise": True, "returnByValue": True}), timeout)
        res = r.get("result", {})
        if "exceptionDetails" in res:
            raise RuntimeError(str(res["exceptionDetails"])[:900])
        return res.get("result", {}).get("value")


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        # 1) anime illustration
        anime = await c.eval("""window.__TAURI__.core.invoke('generate', { req: {
            profile:'qwen21', mode:'t2i',
            prompt:'anime illustration, cel shading, a young woman with long silver hair and red coat, standing in a rainy neon street, full body, detailed',
            negative:'', width:512, height:768, steps:24, cfg:4, seed:12345, batch:1,
            lora:null, loraStrength:1, refImages:[], slg:false
        }, turbo:false}).then(r=>r.images[0].path).catch(e=>'ERR '+e)""")
        print("anime:", anime, flush=True)
        if str(anime).startswith("ERR"):
            return

        # 2) save as reference -> input path
        ref = await c.eval("""(async () => {
            const p = %s;
            const url = await window.__TAURI__.core.invoke('read_output_image', { path: p });
            return window.__TAURI__.core.invoke('save_ref_image', { name: 'anime_ref', dataUrl: url });
        })()""" % json.dumps(anime))
        print("ref:", ref, flush=True)

        # 3) edit with Anything2Real
        real = await c.eval("""window.__TAURI__.core.invoke('generate', { req: {
            profile:'qwen21', mode:'edit',
            prompt:'turn this into a realistic photo of a person, natural skin texture, soft window light, detailed clothing',
            negative:'', width:512, height:768, steps:30, cfg:4, seed:777, batch:1,
            lora:'Qwen2.1_Anything2RealCharacters.safetensors', loraStrength:0.8,
            refImages:[%s], slg:false
        }, turbo:false}).then(r=>r.images[0].path).catch(e=>'ERR '+e)""" % json.dumps(ref))
        print("real:", real, flush=True)

        from PIL import Image
        if not str(real).startswith("ERR"):
            a = Image.open(os.path.join(ROOT, "ComfyUI", "output", os.path.basename(anime)))
            b = Image.open(os.path.join(ROOT, "ComfyUI", "output", os.path.basename(real)))
            h = max(a.height, b.height)
            a = a.resize((int(a.width * h / a.height), h))
            b = b.resize((int(b.width * h / b.height), h))
            canvas = Image.new("RGB", (a.width + b.width + 10, h), (18, 20, 26))
            canvas.paste(a, (0, 0)); canvas.paste(b, (a.width + 10, 0))
            canvas.save(os.path.join(OUT, "shot_anything2real.png"))
            print("saved comparison", flush=True)


asyncio.run(main())

