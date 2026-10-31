"""v0.16: low-resource preset training (384 / rank8 / depth5 / offload) + advanced panel shot."""
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

    async def eval(self, expr, timeout=1800):
        r = await asyncio.wait_for(
            self.call("Runtime.evaluate", {"expression": expr, "awaitPromise": True, "returnByValue": True}),
            timeout,
        )
        res = r.get("result", {})
        if "exceptionDetails" in res:
            raise RuntimeError(str(res["exceptionDetails"])[:800])
        return res.get("result", {}).get("value")

    async def shot(self, path):
        r = await self.call("Page.captureScreenshot", {"format": "png"})
        open(path, "wb").write(base64.b64decode(r["result"]["data"]))


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        await c.eval("document.getElementById('btnLibrary').click()")
        for _ in range(30):
            await asyncio.sleep(1)
            if await c.eval("document.querySelectorAll('.lib-item').length") > 4:
                break
        await c.eval("""(async () => {
            const imgs = [...document.querySelectorAll('.lib-item img')].slice(0, 2);
            const files = [];
            for (const im of imgs) {
                const r = await fetch(im.src);
                const b = await r.blob();
                files.push(new File([b], 'low_' + files.length + '.png', {type: b.type}));
            }
            window.__low = files;
            return files.length;
        })()""")
        await c.eval("document.getElementById('libTrain').click()")
        await asyncio.sleep(0.5)
        await c.eval("""(() => {
            trainState.files = window.__low;
            trainState.src = 'folder';
            document.querySelector('input[name="trainSrc"][value="folder"]').checked = true;
            document.getElementById('trainCaptionRow').hidden = false;
            document.getElementById('trainAutoCaption').checked = false;
            document.getElementById('trainCaption').value = 'architecture photography';
            document.getElementById('trainName').value = 'low_probe';
            document.querySelector('#trainModal details').open = true;
            const btn = [...document.querySelectorAll('#trainPresets button')].find(b => b.textContent.includes('低配'));
            btn.click();
            document.getElementById('trainSteps').value = 4;
            document.querySelector('#trainModal details').open = true;
            return 'ready';
        })()""")
        await asyncio.sleep(0.5)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_train_advanced.png")
        knobs = await c.eval("""({size:document.getElementById('trainSize').value,
            rank:document.getElementById('trainRank').value,
            depth:document.getElementById('trainDepth').value,
            batch:document.getElementById('trainBatch').value,
            grad:document.getElementById('trainGrad').value,
            offload:document.getElementById('trainOffload').checked,
            dtype:document.getElementById('trainDtype').value,
            steps:document.getElementById('trainSteps').value})""")
        print("knobs:", knobs, flush=True)

        await c.eval("document.getElementById('trainGo').click()")
        t0 = time.time()
        last = None
        for _ in range(400):
            await asyncio.sleep(4)
            st = await c.eval("""({btn:document.getElementById('trainGo').textContent,
                txt:document.getElementById('trainProgressText').textContent,
                lora:document.getElementById('lora').value,
                toast:document.getElementById('toast').hidden ? '' : document.getElementById('toast').textContent})""")
            if st != last:
                print(f"  {time.time()-t0:5.0f}s {st}", flush=True)
                last = st
            if not st["btn"].startswith("训练中"):
                break


asyncio.run(main())
