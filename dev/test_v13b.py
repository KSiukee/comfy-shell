"""Populate results, then check scrolling + viewer switching + enhancer."""
import asyncio
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
            raise RuntimeError(str(res["exceptionDetails"])[:900])
        return res.get("result", {}).get("value")


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)

        # three small generations so the grid overflows
        for i in range(8):
            t0 = time.time()
            await c.eval(f"""(() => {{
                document.getElementById('model').value = 'flux3';
                document.getElementById('prompt').value = 'street market at dusk, warm lanterns {i}';
                document.getElementById('steps').value = 8;
                document.getElementById('width').value = 768;
                document.getElementById('height').value = 512;
                document.getElementById('btnGenerate').click();
                return 'started';
            }})()""")
            for _ in range(60):
                await asyncio.sleep(2)
                busy = await c.eval("document.getElementById('btnGenerate').disabled")
                if not busy:
                    break
            print(f"gen{i} ({time.time()-t0:.0f}s) cards=", await c.eval("document.querySelectorAll('.card').length"), flush=True)

        await asyncio.sleep(2)
        layout = await c.eval("""(() => {
            const res = document.getElementById('results');
            const btn = document.getElementById('btnGenerate').getBoundingClientRect();
            const before = res.scrollTop;
            res.scrollTop = 300;
            const moved = res.scrollTop > before;
            const genVisible = btn.top >= 0 && btn.bottom <= window.innerHeight;
            return {cards: document.querySelectorAll('.card').length,
                    scrollable: res.scrollHeight > res.clientHeight + 4,
                    moved, genVisible, genBottom: Math.round(btn.bottom), winH: window.innerHeight};
        })()""")
        print("layout:", layout, flush=True)

        # viewer wheel + arrows
        await c.eval("document.querySelector('.card img').click()")
        await asyncio.sleep(1.2)
        pos0 = await c.eval("document.getElementById('vwPos').textContent")
        await c.eval("document.getElementById('viewerStage').dispatchEvent(new WheelEvent('wheel', {deltaY: 120, bubbles: true, cancelable: true}))")
        await asyncio.sleep(0.7)
        pos1 = await c.eval("document.getElementById('vwPos').textContent")
        await c.eval("document.dispatchEvent(new KeyboardEvent('keydown', {key: 'ArrowRight', bubbles: true}))")
        await asyncio.sleep(0.7)
        pos2 = await c.eval("document.getElementById('vwPos').textContent")
        print("viewer:", pos0, "-> wheel", pos1, "-> arrow", pos2, flush=True)
        await c.eval("document.getElementById('vwClose').click()")

        # enhancer fallback
        t0 = time.time()
        text = await c.eval("""window.__TAURI__.core.invoke('enhance_prompt', {profile: 'qwen21', mode: 't2i',
            prompt: 'a nude woman lying on silk sheets, morning light', refImages: []}).then(t => t).catch(e => 'ERR ' + e)""")
        print(f"enhance ({time.time()-t0:.0f}s):", (text if isinstance(text, str) else str(text))[:500], flush=True)


asyncio.run(main())


