"""End-to-end UI test: generate one image, then use it as a reference for an edit."""
import asyncio
import base64
import json
import sys
import time
import urllib.request

import websockets


def page_ws():
    data = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json", timeout=5).read())
    for p in data:
        if p.get("type") == "page":
            return p["webSocketDebuggerUrl"]
    raise SystemExit("no page")


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

    async def eval(self, expr):
        r = await self.call("Runtime.evaluate", {"expression": expr, "awaitPromise": True, "returnByValue": True})
        res = r.get("result", {})
        if "exceptionDetails" in res:
            raise RuntimeError(str(res["exceptionDetails"])[:600])
        return res.get("result", {}).get("value")

    async def shot(self, path):
        r = await self.call("Page.captureScreenshot", {"format": "png"})
        open(path, "wb").write(base64.b64decode(r["result"]["data"]))
        print("shot:", path, flush=True)


STATE = """(() => ({
  gen: document.getElementById('btnGenerate').disabled,
  cards: document.querySelectorAll('.card').length,
  prog: document.getElementById('progressText').textContent,
  toast: document.getElementById('toast').hidden ? '' : document.getElementById('toast').textContent,
  mode: document.querySelector('.tab.active').dataset.mode,
  refs: document.querySelectorAll('.thumb').length,
  status: document.getElementById('statusText').textContent
}))()"""


async def wait_idle(c, timeout=300):
    t0 = time.time()
    while time.time() - t0 < timeout:
        st = await c.eval(STATE)
        if not st["gen"]:
            return st
        await asyncio.sleep(2)
    raise TimeoutError("generation did not finish")


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        await c.call("Runtime.enable")

        # --- 1. text to image through the UI
        prompt = "一只红熊猫坐在窗边的木桌上修一盏铜制台灯，午后斜光，尘埃在光柱里漂浮，50mm 定焦，浅景深"
        await c.eval(f"""(() => {{
            const el = document.getElementById('prompt');
            el.value = {json.dumps(prompt)};
            el.dispatchEvent(new Event('input', {{bubbles:true}}));
            document.getElementById('seed').value = 1234;
            document.getElementById('btnGenerate').click();
            return 'clicked';
        }})()""")
        print("t2i submitted", flush=True)
        t0 = time.time()
        st = await wait_idle(c)
        print("t2i done in %.0fs ->" % (time.time() - t0), st, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_t2i.png")

        # --- 2. reuse the result as a reference and edit
        await c.eval("document.querySelector('.card img').click()")
        await asyncio.sleep(0.5)
        await c.eval("document.getElementById('lightboxRef').click()")
        await asyncio.sleep(1.0)
        st = await c.eval(STATE)
        print("after ref:", st, flush=True)
        await c.eval(f"""(() => {{
            const el = document.getElementById('prompt');
            el.value = {json.dumps("把画面改成水彩画风格，保留红熊猫、台灯和构图，纸张纹理可见，暖色调")};
            el.dispatchEvent(new Event('input', {{bubbles:true}}));
            document.getElementById('btnGenerate').click();
            return 'clicked';
        }})()""")
        print("edit submitted", flush=True)
        t0 = time.time()
        st = await wait_idle(c)
        print("edit done in %.0fs ->" % (time.time() - t0), st, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_edit.png")


asyncio.run(main())
