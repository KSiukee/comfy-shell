"""v0.12 final: 640 + keep-aspect (mirror pad) training on real photos, 4 steps."""
import asyncio
import base64
import json
import time
import urllib.request

import websockets


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

    async def eval(self, expr, timeout=3600):
        r = await asyncio.wait_for(
            self.call("Runtime.evaluate", {"expression": expr, "awaitPromise": True, "returnByValue": True}),
            timeout,
        )
        res = r.get("result", {})
        if "exceptionDetails" in res:
            raise RuntimeError(str(res["exceptionDetails"])[:900])
        return res.get("result", {}).get("value")

    async def shot(self, path):
        r = await self.call("Page.captureScreenshot", {"format": "png"})
        open(path, "wb").write(base64.b64decode(r["result"]["data"]))


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        await c.call("Runtime.enable")
        await c.eval("document.getElementById('btnLibrary').click()")
        for _ in range(30):
            await asyncio.sleep(1)
            if await c.eval("document.querySelectorAll('.lib-item').length") > 8:
                break
        n = await c.eval("""(async () => {
            const imgs = [...document.querySelectorAll('.lib-item img')]
                .filter(im => im.alt && im.alt.startsWith('comfyshell')).slice(0, 2);
            const files = [];
            for (const im of imgs) {
                const r = await fetch(im.src); const b = await r.blob();
                files.push(new File([b], 'photo_' + files.length + '.png', {type: b.type}));
            }
            window.__photos = files;
            return files.length;
        })()""")
        print("photos:", n, flush=True)
        await c.eval("document.getElementById('libTrain').click()")
        await asyncio.sleep(0.5)
        await c.eval("""(() => {
            trainState.files = window.__photos;
            trainState.src = 'folder';
            document.querySelector('input[name="trainSrc"][value="folder"]').checked = true;
            document.getElementById('trainCaptionRow').hidden = false;
            document.getElementById('trainAutoCaption').checked = false;
            document.getElementById('trainCaption').value = 'architecture photography, warm evening light';
            document.getElementById('trainSize').value = '640';
            document.getElementById('trainAspect').value = 'keep';
            document.getElementById('trainName').value = 'mirror_probe';
            document.getElementById('trainSteps').value = 4;
            trainRefreshSource();
            document.getElementById('trainGo').click();
        })()""")
        t0 = time.time()
        last = None
        for _ in range(300):
            await asyncio.sleep(5)
            st = await c.eval("""({btn:document.getElementById('trainGo').textContent,
                bar:document.getElementById('trainBar').style.width,
                txt:document.getElementById('trainProgressText').textContent,
                lora:document.getElementById('lora').value,
                toast:document.getElementById('toast').hidden ? '' : document.getElementById('toast').textContent})""")
            if st != last:
                print(f"  {time.time()-t0:5.0f}s {st}", flush=True)
                last = st
            if not st["btn"].startswith("训练中"):
                break
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_640.png")


asyncio.run(main())
