"""v0.13 final checks: scroll, star toggle, footer look, lightbox switching."""
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

    async def eval(self, expr, timeout=900):
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


async def gen(c, prompt):
    await c.eval(f"""(() => {{
        document.getElementById('model').value = 'flux3';
        document.getElementById('prompt').value = {json.dumps(prompt)};
        document.getElementById('steps').value = 8;
        document.getElementById('width').value = 768;
        document.getElementById('height').value = 512;
        document.getElementById('btnGenerate').click();
        return 'go';
    }})()""")
    for _ in range(60):
        await asyncio.sleep(2)
        if not await c.eval("document.getElementById('btnGenerate').disabled"):
            break


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        for i in range(6):
            await gen(c, f"street market at dusk, warm lanterns, shot {i}")
        print("cards:", await c.eval("document.querySelectorAll('.card').length"), flush=True)

        layout = await c.eval("""(() => {
            const res = document.getElementById('results');
            const before = res.scrollTop;
            res.scrollTop = 400;
            const btn = document.getElementById('btnGenerate').getBoundingClientRect();
            const lo = document.getElementById('lora').getBoundingClientRect();
            return {scrollable: res.scrollHeight > res.clientHeight + 4,
                    moved: res.scrollTop > before,
                    cardH: Math.round(document.querySelector('.card').getBoundingClientRect().height),
                    genRect: [Math.round(btn.top), Math.round(btn.bottom)],
                    loraBottom: Math.round(lo.bottom), winH: window.innerHeight};
        })()""")
        print("layout:", layout, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_footer.png")

        # quick star on the first card
        star = await c.eval("""(() => {
            const card = document.querySelector('.card');
            const b = card.querySelector('button.star');
            b.click();
            return {label: b.textContent, on: b.classList.contains('on'), path: card.querySelector('img').dataset.path};
        })()""")
        await asyncio.sleep(1.2)
        persisted = await c.eval("window.__TAURI__.core.invoke('stars_list')")
        print("star:", star, "| persisted:", star.get("path") in (persisted or []), flush=True)

        # lightbox: wheel + arrows
        await c.eval("document.querySelector('.card img').click()")
        await asyncio.sleep(1.2)
        p0 = await c.eval("[document.getElementById('lbPos').textContent, document.getElementById('lightbox').hidden]")
        await c.eval("document.getElementById('lightbox').dispatchEvent(new WheelEvent('wheel', {deltaY: 120, bubbles: true, cancelable: true}))")
        await asyncio.sleep(0.7)
        p1 = await c.eval("document.getElementById('lbPos').textContent")
        await c.eval("document.dispatchEvent(new KeyboardEvent('keydown', {key: 'ArrowRight', bubbles: true}))")
        await asyncio.sleep(0.7)
        p2 = await c.eval("document.getElementById('lbPos').textContent")
        print("lightbox:", p0, "-> wheel", p1, "-> arrow", p2, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_lightbox_nav.png")
        await c.eval("document.getElementById('lightboxClose').click()")

        # enhancer refusal path (graphic horror usually gets refused by the official PE)
        text = await c.eval("""window.__TAURI__.core.invoke('enhance_prompt', {profile: 'qwen21', mode: 't2i',
            prompt: 'graphic gore, a dismembered bloody corpse on a wet floor', refImages: []})
            .then(t => t).catch(e => 'ERR ' + e)""")
        print("gore enhance:", (text if isinstance(text, str) else str(text))[:260], flush=True)


asyncio.run(main())
