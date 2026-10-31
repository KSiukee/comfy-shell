"""v0.14 checks: step presets, star, footer, free-enhance setting."""
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


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        await asyncio.sleep(2)
        presets = await c.eval("([...document.querySelectorAll('#stepPresets button')].map(b=>b.textContent))")
        click = await c.eval("""(() => {
            const b = [...document.querySelectorAll('#stepPresets button')].find(x => x.textContent === '30');
            b.click();
            return {steps: document.getElementById('steps').value, on: b.classList.contains('on')};
        })()""")
        print("presets:", presets, "| click 30:", click, flush=True)

        # footer look + star button presence
        ui = await c.eval("""(() => ({
            cards: document.querySelectorAll('.card').length,
            stars: document.querySelectorAll('.card button.star').length,
            starred: document.querySelectorAll('.card button.star.on').length,
            genTop: Math.round(document.getElementById('btnGenerate').getBoundingClientRect().top),
        }))()""")
        print("ui:", ui, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_steps.png")

        # free enhancer setting on -> output should come from the heretic rewriter
        await c.eval("""window.__TAURI__.core.invoke('settings_get').then(s => {
            s.freeEnhance = true;
            return window.__TAURI__.core.invoke('settings_set', {settings: s});
        })""")
        t0 = time.time()
        text = await c.eval("""window.__TAURI__.core.invoke('enhance_prompt', {profile: 'qwen21', mode: 't2i',
            prompt: 'a nude woman lying on silk sheets, morning light', refImages: []})
            .then(t => t).catch(e => 'ERR ' + e)""")
        print(f"free enhance ({time.time()-t0:.0f}s):", str(text)[:260], flush=True)
        await c.eval("""window.__TAURI__.core.invoke('settings_get').then(s => {
            s.freeEnhance = false;
            return window.__TAURI__.core.invoke('settings_set', {settings: s});
        })""")


asyncio.run(main())
