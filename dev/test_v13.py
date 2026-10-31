"""v0.13: layout scroll, right-click delete, viewer wheel/keys, uncensored enhancer fallback."""
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

    async def eval(self, expr, timeout=600):
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
        await asyncio.sleep(2)

        # --- layout: results scroll + generate button reachable
        layout = await c.eval("""(() => {
            const res = document.getElementById('results');
            const btn = document.getElementById('btnGenerate').getBoundingClientRect();
            const before = res.scrollTop;
            res.scrollTop = 200;
            return {cards: document.querySelectorAll('.card').length,
                    scrollable: res.scrollHeight > res.clientHeight + 4,
                    moved: res.scrollTop > before,
                    genVisible: btn.top >= 0 && btn.bottom <= window.innerHeight,
                    genBottom: Math.round(btn.bottom), winH: window.innerHeight};
        })()""")
        print("layout:", layout, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_layout.png")

        # --- right-click menu on the first card
        menu = await c.eval("""(() => {
            const card = document.querySelector('.card');
            const r = card.getBoundingClientRect();
            card.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, clientX: r.left + 60, clientY: r.top + 60}));
            const m = document.getElementById('ctxmenu');
            return {open: !m.hidden, items: [...m.querySelectorAll('button')].map(b => b.textContent)};
        })()""")
        print("ctx menu:", menu, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_ctxmenu.png")

        # --- delete through the menu
        before = await c.eval("document.querySelectorAll('.card').length")
        deleted = await c.eval("""(() => {
            const items = [...document.querySelectorAll('#ctxmenu button')];
            const del = items.find(b => b.textContent.includes('删除'));
            del.click();
            return 'clicked';
        })()""")
        await asyncio.sleep(1.5)
        after = await c.eval("document.querySelectorAll('.card').length")
        print("delete:", deleted, before, "->", after, flush=True)

        # --- viewer: wheel + arrows switch images
        await c.eval("document.querySelector('.card img').click()")
        await asyncio.sleep(1.5)
        pos0 = await c.eval("document.getElementById('vwPos').textContent")
        await c.eval("""(() => {
            const s = document.getElementById('viewerStage');
            s.dispatchEvent(new WheelEvent('wheel', {deltaY: 120, bubbles: true, cancelable: true}));
        })()""")
        await asyncio.sleep(0.8)
        pos1 = await c.eval("document.getElementById('vwPos').textContent")
        await c.eval("document.dispatchEvent(new KeyboardEvent('keydown', {key: 'ArrowRight', bubbles: true}))")
        await asyncio.sleep(0.8)
        pos2 = await c.eval("document.getElementById('vwPos').textContent")
        print("viewer:", pos0, "-> wheel", pos1, "-> arrow", pos2, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_viewer_switch.png")
        await c.eval("document.getElementById('vwClose').click()")

        # --- uncensored enhancer
        t0 = time.time()
        text = await c.eval("""window.__TAURI__.core.invoke('enhance_prompt', {profile: 'qwen21', mode: 't2i',
            prompt: 'a nude woman lying on silk sheets, morning light', refs: []}).then(t => t).catch(e => 'ERR ' + e)""", 300)
        print(f"enhance ({time.time()-t0:.0f}s):", (text if isinstance(text, str) else str(text))[:400], flush=True)


asyncio.run(main())
