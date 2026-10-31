"""Final v0.3 check: reference-guided t2i honours the size control (pixel area)."""
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

    async def eval(self, expr, timeout=180):
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
        print("boot:", await c.eval("({status:document.getElementById('statusText').textContent, cards:document.querySelectorAll('.card').length})"), flush=True)

        # pick the model FIRST (changing it clears refs), then attach a reference
        await c.eval("""(() => {
            const s = document.getElementById('model');
            s.value = 'flux3'; s.dispatchEvent(new Event('change', {bubbles:true}));
        })()""")
        await asyncio.sleep(0.5)
        await c.eval("document.querySelector('.card img').click()")
        await asyncio.sleep(1.2)
        await c.eval("document.getElementById('lightboxRef').click()")
        await asyncio.sleep(1.2)
        await c.eval("""(() => {
            document.querySelector('.tab[data-mode="t2i"]').click();
            document.getElementById('width').value = 768;
            document.getElementById('height').value = 512;
            const el = document.getElementById('prompt');
            el.value = 'the same subject, now standing on a cobblestone street at night, rain';
            el.dispatchEvent(new Event('input', {bubbles:true}));
            document.getElementById('btnGenerate').click();
        })()""")
        g = None
        for _ in range(60):
            await asyncio.sleep(2)
            g = await c.eval("({gen:document.getElementById('btnGenerate').disabled, cards:document.querySelectorAll('.card').length, toast:document.getElementById('toast').hidden?'':document.getElementById('toast').textContent})")
            if not g["gen"]:
                break
        print("t2i+ref 768x512:", g, flush=True)
        dims = await c.eval("""(() => {
            const card = document.querySelector('.card .meta .time');
            const img = document.querySelector('.card img');
            return {meta: card ? card.textContent.trim() : '', natural: img ? img.naturalWidth + 'x' + img.naturalHeight : ''};
        })()""")
        print("newest card:", dims, flush=True)
        await c.eval("document.getElementById('toast').hidden = true")
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\screenshot.png")


asyncio.run(main())

