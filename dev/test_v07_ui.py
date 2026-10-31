"""v0.7 UI test: select images -> 训练 LoRA -> name/steps -> wait -> LoRA registered & selected."""
import asyncio
import base64
import json
import time
import urllib.request

import websockets


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

    async def eval(self, expr, timeout=3600):
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
        await c.call("Runtime.enable")
        print("boot:", await c.eval("({status:document.getElementById('statusText').textContent})"), flush=True)

        # open library, pick two images
        await c.eval("document.getElementById('btnLibrary').click()")
        for _ in range(30):
            await asyncio.sleep(1)
            if await c.eval("document.querySelectorAll('.lib-item').length") > 2:
                break
        await c.eval("""(() => {
            const b=[...document.querySelectorAll('.lib-item input[type=checkbox]')];
            b[0].click(); b[1].click();
        })()""")
        await asyncio.sleep(0.5)

        # open training modal
        await c.eval("document.getElementById('libTrain').click()")
        await asyncio.sleep(0.6)
        state = await c.eval("({count:document.getElementById('trainCount').textContent, modal:!document.getElementById('trainModal').hidden})")
        print("train modal:", state, flush=True)

        # name + small steps, start
        await c.eval("""(() => {
            document.getElementById('trainName').value = 'ui_probe_3';
            document.getElementById('trainSteps').value = 16;
            document.getElementById('trainRank').value = 8;
            document.getElementById('trainGo').click();
        })()""")
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_train.png")

        t0 = time.time()
        res = None
        for _ in range(600):
            await asyncio.sleep(5)
            res = await c.eval("""({btn: document.getElementById('trainGo').textContent,
                modal: !document.getElementById('trainModal').hidden,
                toast: document.getElementById('toast').hidden ? '' : document.getElementById('toast').textContent,
                lora: document.getElementById('lora').value})""")
            if not res["btn"].startswith("训练中"):
                break
        print(f"train done ({time.time()-t0:.0f}s):", res, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_trained.png")


asyncio.run(main())



