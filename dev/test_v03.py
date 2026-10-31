"""v0.3 feature test: library, compare, batch queue, t2i with reference image."""
import asyncio
import base64
import json
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
        print("boot:", await c.eval("({status:document.getElementById('statusText').textContent, cards:document.querySelectorAll('.card').length, refVisible:!document.getElementById('refField').hidden})"), flush=True)

        # 1. library
        await c.eval("document.getElementById('btnLibrary').click()")
        info = None
        for _ in range(30):
            await asyncio.sleep(1)
            info = await c.eval("({items:document.querySelectorAll('.lib-item').length, count:document.getElementById('libCount').textContent, open:!document.getElementById('library').hidden})")
            if info["items"] > 0:
                break
        print("library:", info, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_library.png")

        # 2. select two and compare
        await c.eval("""(() => {
            const boxes = [...document.querySelectorAll('.lib-item input[type=checkbox]')];
            boxes[0].click(); boxes[1].click();
            document.getElementById('libCompare').click();
        })()""")
        await asyncio.sleep(2.5)
        cmp = await c.eval("({open:!document.getElementById('compare').hidden, a:document.getElementById('cmpALabel').textContent, b:document.getElementById('cmpBLabel').textContent, zoom:document.getElementById('cmpZoomText').textContent})")
        print("compare:", cmp, flush=True)
        await c.eval("document.getElementById('cmpZoom').value=160; document.getElementById('cmpZoom').dispatchEvent(new Event('input'))")
        await asyncio.sleep(0.6)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_compare.png")
        await c.eval("document.getElementById('cmpClose').click(); document.getElementById('libClose').click()")

        # 3. batch queue (2 prompts, current model = qwen21? set flux3 for speed)
        await c.eval("""(() => {
            const s = document.getElementById('model');
            s.value = 'flux3'; s.dispatchEvent(new Event('change', {bubbles:true}));
            document.getElementById('queueText').value = 'a lighthouse in a storm, oil painting\\na cozy bookshop at night, warm lamps';
            document.getElementById('btnQueueAdd').click();
            document.getElementById('btnQueueRun').click();
        })()""")
        q = None
        for _ in range(120):
            await asyncio.sleep(2)
            q = await c.eval("({state:document.getElementById('queueState').textContent, count:document.getElementById('queueCount').textContent, runHidden:document.getElementById('btnQueueRun').hidden, cards:document.querySelectorAll('.card').length})")
            if not q["state"]:
                break
        print("queue:", q, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_queue.png")

        # 4. t2i with a reference image (reuse last result), then switch back to 生图
        await c.eval("document.querySelector('.card img').click()")
        await asyncio.sleep(1.2)
        await c.eval("document.getElementById('lightboxRef').click()")
        await asyncio.sleep(1.2)
        await c.eval("""(() => {
            document.querySelector('.tab[data-mode="t2i"]').click();
            const el = document.getElementById('prompt');
            el.value = 'same subject, now in a snowy mountain village at dusk';
            el.dispatchEvent(new Event('input', {bubbles:true}));
            document.getElementById('btnGenerate').click();
        })()""")
        g = None
        for _ in range(60):
            await asyncio.sleep(2)
            g = await c.eval("({gen:document.getElementById('btnGenerate').disabled, cards:document.querySelectorAll('.card').length, toast:document.getElementById('toast').hidden?'':document.getElementById('toast').textContent, mode:document.querySelector('.tab.active').dataset.mode, refs:document.querySelectorAll('.thumb').length})")
            if not g["gen"]:
                break
        print("t2i+ref:", g, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_t2i_ref.png")


asyncio.run(main())
