import asyncio, json, os, urllib.request
import websockets
from PIL import Image, ImageDraw

ROOT = r"D:\deepseek-files\ai-tools\ComfyUI_windows_portable"
PROJ = r"D:\deepseek-files\ai-tools\comfy-shell"
PROMPT = ("a young woman with short black hair and round glasses, wearing a beige trench coat and black trousers, "
          "photorealistic, natural window light, full body")
POSES = ["P001", "P009", "P015"]


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
            raise RuntimeError(str(res["exceptionDetails"])[:600])
        return res.get("result", {}).get("value")


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        catalog = await c.eval("window.__TAURI__.core.invoke('poses_catalog').then(s=>JSON.parse(s))")
        by_id = {p["id"]: p for p in catalog}
        results = []
        for pid in POSES:
            pose = by_id.get(pid)
            if not pose:
                print(pid, "missing", flush=True)
                continue
            views = pose.get("views") or {}
            file = views.get("front") or views.get("diag") or next(iter(views.values()))
            ref = await c.eval("""(async () => {
                const url = await window.__TAURI__.core.invoke('read_input_image', { name: %s });
                return window.__TAURI__.core.invoke('save_ref_image', { name: %s, dataUrl: url });
            })()""" % (json.dumps(f"shisei/line/{file}"), json.dumps(f"posetest_{pid}.png")))
            out = await c.eval("""window.__TAURI__.core.invoke('generate', { req: {
                profile:'qwen21', mode:'edit', prompt:%s, negative:'', width:512, height:768,
                steps:30, cfg:4, seed:4242, batch:1, lora:null, loraStrength:1,
                refImages:[%s], slg:false
            }, turbo:false}).then(r=>r.images[0].path).catch(e=>'ERR '+e)""" % (json.dumps(PROMPT), json.dumps(ref)))
            print(pid, file, "->", out, flush=True)
            results.append((f"{pid} {pose.get('name_cn','')}", out))

        imgs = []
        for label, path in results:
            if path and not str(path).startswith("ERR") and os.path.exists(path):
                imgs.append((label, Image.open(path).convert("RGB")))
        if imgs:
            w, h = 512, 768
            canvas = Image.new("RGB", (w * len(imgs) + 20 * (len(imgs) + 1), h + 70), (16, 18, 24))
            d = ImageDraw.Draw(canvas)
            for i, (label, im) in enumerate(imgs):
                x = 20 + i * (w + 20)
                canvas.paste(im.resize((w, h)), (x, 40))
                d.text((x + 6, 12), label, fill=(255, 255, 255))
            canvas.save(os.path.join(PROJ, "shot_pose_transfer.png"))
            print("saved", len(imgs), flush=True)


asyncio.run(main())
