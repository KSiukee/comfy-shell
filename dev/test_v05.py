"""v0.5 test: CN->EN search translation, viewer (zoom/arrow/upscale)."""
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

    async def eval(self, expr, timeout=240):
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

        await c.eval("document.getElementById('btnLibrary').click()")
        for _ in range(30):
            await asyncio.sleep(1)
            if await c.eval("document.querySelectorAll('.lib-item').length") > 0:
                break
        total = await c.eval("document.getElementById('libCount').textContent")

        # --- 1. chinese query -> 0 hits, then translate and search again
        await c.eval("""(() => { const s=document.getElementById('libSearch'); s.value='柴犬'; s.dispatchEvent(new Event('input')); })()""")
        await asyncio.sleep(0.4)
        before = await c.eval("document.getElementById('libCount').textContent")
        await c.eval("document.getElementById('libTranslate').click()")
        for _ in range(60):
            await asyncio.sleep(2)
            st = await c.eval("({busy:document.getElementById('libTranslate').textContent, chip:!document.getElementById('libAltChip').hidden, alt:document.getElementById('libAltText').textContent, count:document.getElementById('libCount').textContent})")
            if not st["busy"].startswith("翻译"):
                break
        print("translate search:", total, "| zh:", before, "->", st, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_translate.png")

        # clear filter and open the first image in the viewer
        await c.eval("document.getElementById('libAltClear').click(); document.getElementById('libSearch').value=''; document.getElementById('libSearch').dispatchEvent(new Event('input'));")
        await asyncio.sleep(0.4)
        await c.eval("document.querySelector('.lib-item img').click()")
        await asyncio.sleep(2)
        v = await c.eval("({open:!document.getElementById('viewer').hidden, pos:document.getElementById('vwPos').textContent, zoom:document.getElementById('vwZoom').textContent, name:document.getElementById('vwName').textContent})")
        print("viewer:", v, flush=True)

        # zoom in and go next
        await c.eval("document.getElementById('vwZoomIn').click(); document.getElementById('vwZoomIn').click();")
        await asyncio.sleep(0.3)
        await c.eval("document.getElementById('vwNext').click()")
        await asyncio.sleep(1.5)
        v2 = await c.eval("({pos:document.getElementById('vwPos').textContent, zoom:document.getElementById('vwZoom').textContent, name:document.getElementById('vwName').textContent})")
        print("after next:", v2, flush=True)

        # keyboard arrows
        await c.eval("document.dispatchEvent(new KeyboardEvent('keydown', {key:'ArrowRight', bubbles:true}))")
        await asyncio.sleep(1.2)
        print("after arrow:", await c.eval("document.getElementById('vwPos').textContent"), flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_viewer.png")

        # upscale current image
        await c.eval("document.getElementById('vwUpscale').click()")
        up = None
        for _ in range(90):
            await asyncio.sleep(2)
            up = await c.eval("({busy:document.getElementById('vwUpscale').textContent, name:document.getElementById('vwName').textContent, toast:document.getElementById('toast').hidden?'':document.getElementById('toast').textContent})")
            if not up["busy"].startswith("超分中"):
                break
        print("upscale:", up, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_upscale.png")


asyncio.run(main())
