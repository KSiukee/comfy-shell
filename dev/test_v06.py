"""v0.6 test: mask painting -> local redraw (inpaint), plus upscale model selector."""
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

    async def eval(self, expr, timeout=300):
        r = await asyncio.wait_for(
            self.call("Runtime.evaluate", {"expression": expr, "awaitPromise": True, "returnByValue": True}),
            timeout,
        )
        res = r.get("result", {})
        if "exceptionDetails" in res:
            raise RuntimeError(str(res["exceptionDetails"])[:700])
        return res.get("result", {}).get("value")

    async def shot(self, path):
        r = await self.call("Page.captureScreenshot", {"format": "png"})
        open(path, "wb").write(base64.b64decode(r["result"]["data"]))


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        await c.call("Runtime.enable")
        print("boot:", await c.eval("({status:document.getElementById('statusText').textContent, cards:document.querySelectorAll('.card').length})"), flush=True)

        # open library + viewer, pick a fairly large image
        await c.eval("document.getElementById('btnLibrary').click()")
        for _ in range(30):
            await asyncio.sleep(1)
            if await c.eval("document.querySelectorAll('.lib-item').length") > 0:
                break
        await c.eval("document.querySelector('.lib-item img').click()")
        await asyncio.sleep(2.5)
        info = await c.eval("({pos:document.getElementById('vwPos').textContent, name:document.getElementById('vwName').textContent, upModels:[...document.getElementById('vwUpModel').options].map(o=>o.value)})")
        print("viewer:", info, flush=True)

        # enter mask mode and paint a blob in the middle
        await c.eval("document.getElementById('vwInpaint').click()")
        await asyncio.sleep(0.8)
        painted = await c.eval("""(() => {
            const c = document.getElementById('maskCanvas');
            const r = c.getBoundingClientRect();
            const mk = (t,x,y) => new PointerEvent(t, {clientX:r.left+r.width*x, clientY:r.top+r.height*y, bubbles:true, pointerId:1});
            c.dispatchEvent(mk('pointerdown',0.42,0.40));
            for (let i=1;i<=12;i++) c.dispatchEvent(mk('pointermove',0.42+i*0.012,0.40+i*0.014));
            c.dispatchEvent(mk('pointerup',0.56,0.56));
            // count painted pixels
            const ctx = c.getContext('2d');
            const d = ctx.getImageData(0,0,c.width,c.height).data;
            let lit = 0;
            for (let i=0;i<d.length;i+=4) if (d[i] > 128) lit++;
            return {maskOpen: !c.hidden, w: c.width, h: c.height, lit};
        })()""")
        print("mask painted:", painted, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_mask.png")

        # start redraw, fill prompt, run
        await c.eval("document.getElementById('maskStart').click()")
        await asyncio.sleep(0.5)
        await c.eval("""(() => {
            document.getElementById('inpaintPrompt').value = '一朵盛开的红玫瑰 / a blooming red rose';
            document.getElementById('inpaintGo').click();
        })()""")
        res = None
        for _ in range(120):
            await asyncio.sleep(2)
            res = await c.eval("""({busy: document.getElementById('inpaintGo').textContent,
                modal: !document.getElementById('inpaintModal').hidden,
                toast: document.getElementById('toast').hidden ? '' : document.getElementById('toast').textContent,
                name: document.getElementById('vwName').textContent,
                cards: document.querySelectorAll('.card').length})""")
            if not res["busy"].startswith("重绘"):
                break
        print("inpaint:", res, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_inpaint.png")


asyncio.run(main())
