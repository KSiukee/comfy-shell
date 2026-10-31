"""v0.4 test (sequential): library search, compare export, batch edit, model sweep."""
import asyncio
import base64
import glob
import json
import os
import urllib.request

import websockets

OUT = r"D:\deepseek-files\ai-tools\ComfyUI_windows_portable\ComfyUI\output"


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
        cards0 = await c.eval("document.querySelectorAll('.card').length")

        # --- library search by prompt
        await c.eval("document.getElementById('btnLibrary').click()")
        for _ in range(30):
            await asyncio.sleep(1)
            if await c.eval("document.querySelectorAll('.lib-item').length") > 0:
                break
        await c.eval("""(() => { const s=document.getElementById('libSearch'); s.value='Shiba'; s.dispatchEvent(new Event('input')); })()""")
        await asyncio.sleep(0.4)
        print("search 'Shiba':", await c.eval("document.getElementById('libCount').textContent"), flush=True)
        await c.eval("""(() => { const s=document.getElementById('libSearch'); s.value=''; s.dispatchEvent(new Event('input')); })()""")

        # --- compare + export
        await c.eval("""(() => {
            const b=[...document.querySelectorAll('.lib-item input[type=checkbox]')];
            b[0].click(); b[1].click();
            document.getElementById('libCompare').click();
        })()""")
        await asyncio.sleep(2.5)
        await c.eval("document.getElementById('cmpExport').click()")
        await asyncio.sleep(2)
        exported = sorted(glob.glob(os.path.join(OUT, "compare_*.png")))
        print("compare export:", os.path.basename(exported[-1]) if exported else "MISSING", flush=True)
        await c.eval("document.getElementById('cmpClose').click()")

        # --- batch edit the two selected images (waits for modal to close)
        await c.eval("document.getElementById('libEdit').click()")
        await asyncio.sleep(0.5)
        await c.eval("""(() => {
            document.getElementById('batchEditPrompt').value = '改成水彩画风格，保留主体和构图，纸张纹理可见';
            document.getElementById('batchEditStart').click();
        })()""")
        for _ in range(180):
            await asyncio.sleep(2)
            st = await c.eval("({modal:!document.getElementById('batchEditModal').hidden, cards:document.querySelectorAll('.card').length})")
            if not st["modal"]:
                break
        after = await c.eval("""({cards: document.querySelectorAll('.card').length,
            labels: [...document.querySelectorAll('.card .meta .label')].slice(0,3).map(e=>e.textContent)})""")
        print("batch edit:", after, "delta:", after["cards"] - cards0, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_batch.png")

        # --- model sweep on the current prompt (2 models to stay quick)
        prompt = "a quiet street market at dawn, warm light, 35mm"
        await c.eval(f"""(() => {{
            const boxes=[...document.querySelectorAll('#sweepModels input')];
            boxes.forEach(b => {{ b.checked = ['qwen21','flux3'].includes(b.value); }});
            document.getElementById('width').value = 768;
            document.getElementById('height').value = 512;
            document.getElementById('seed').value = 42;
            const p=document.getElementById('prompt'); p.value={json.dumps(prompt)};
            p.dispatchEvent(new Event('input', {{bubbles:true}}));
            document.getElementById('btnSweepRun').click();
        }})()""")
        for _ in range(240):
            await asyncio.sleep(3)
            st = await c.eval("({btn:document.getElementById('btnSweepRun').disabled, state:document.getElementById('sweepState').textContent})")
            if not st["btn"]:
                break
        final = await c.eval("""({cards: document.querySelectorAll('.card').length,
            labels: [...document.querySelectorAll('.card .meta .label')].slice(0,4).map(e=>e.textContent),
            sizes: [...document.querySelectorAll('.card .meta .time')].slice(0,4).map(e=>e.textContent)})""")
        print("sweep done:", final, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_sweep.png")


asyncio.run(main())
