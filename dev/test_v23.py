"""v0.23 checks: shift node, turbo hidden, favourites mirror, lora2 chain, UI layout."""
import asyncio
import json
import time
import urllib.request
import os

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
            raise RuntimeError(str(res["exceptionDetails"])[:800])
        return res.get("result", {}).get("value")

    async def shot(self, path):
        r = await self.call("Page.captureScreenshot", {"format": "png"})
        open(path, "wb").write(base64.b64decode(r["result"]["data"]))


import base64


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        ui = await c.eval("""({
            turboHidden: document.getElementById('turbo').parentElement.hidden,
            cfgInline: !!document.getElementById('steps').parentElement.querySelector('#cfg'),
            libSize: !!document.getElementById('libSize'),
            lora2: !!document.getElementById('lora2')
        })""")
        print("ui:", ui, flush=True)

        # star -> favourites mirror
        path = await c.eval("window.__TAURI__.core.invoke('library_scan').then(l=>l[0].path)")
        await c.eval("window.__TAURI__.core.invoke('library_star', {path: " + json.dumps(path) + ", starred: true})")
        fav = r"D:\deepseek-files\ai-tools\ComfyUI_windows_portable\ComfyUI\output\_favorites"
        name = os.path.basename(path)
        print("favourite file exists:", os.path.exists(os.path.join(fav, name)), flush=True)
        await c.eval("window.__TAURI__.core.invoke('library_star', {path: " + json.dumps(path) + ", starred: false})")
        print("favourite removed:", not os.path.exists(os.path.join(fav, name)), flush=True)

        # moss generation with the shift node
        await c.eval("""(() => {
            document.getElementById('model').value='moss';
            document.getElementById('model').dispatchEvent(new Event('change'));
            document.getElementById('prompt').value='a cat on a windowsill, morning light';
            document.getElementById('steps').value=8; document.getElementById('width').value=512; document.getElementById('height').value=512;
            document.getElementById('btnGenerate').click(); return 'go';
        })()""")
        for _ in range(60):
            await asyncio.sleep(2)
            if not await c.eval("document.getElementById('btnGenerate').disabled"):
                break
        print("moss gen cards:", await c.eval("document.querySelectorAll('.card').length"),
              "toast:", await c.eval("document.getElementById('toast').hidden?'':document.getElementById('toast').textContent"), flush=True)

        # two-LoRA chain
        await c.eval("""(() => {
            const l2=document.getElementById('lora2');
            const opts=[...l2.options].map(o=>o.value).filter(v=>v.includes('kohya'));
            if(opts.length>=2){ l2.value=opts[0]; l2.dispatchEvent(new Event('change'));
              const l=document.getElementById('lora'); l.value=opts[1]; l.dispatchEvent(new Event('change')); }
            document.getElementById('steps').value=4;
            document.getElementById('btnGenerate').click(); return opts;
        })()""")
        for _ in range(60):
            await asyncio.sleep(2)
            if not await c.eval("document.getElementById('btnGenerate').disabled"):
                break
        print("two-lora gen:", await c.eval("({cards:document.querySelectorAll('.card').length, toast:document.getElementById('toast').hidden?'':document.getElementById('toast').textContent})"), flush=True)

        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\screenshot.png")


asyncio.run(main())
