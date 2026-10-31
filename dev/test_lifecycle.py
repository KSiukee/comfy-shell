"""Full lifecycle check for ComfyShell: start, stop, restart, generate, edit."""
import asyncio
import base64
import json
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

    async def eval(self, expr, timeout=30):
        r = await asyncio.wait_for(
            self.call("Runtime.evaluate", {"expression": expr, "awaitPromise": True, "returnByValue": True}),
            timeout,
        )
        res = r.get("result", {})
        if "exceptionDetails" in res:
            raise RuntimeError(str(res["exceptionDetails"])[:500])
        return res.get("result", {}).get("value")

    async def shot(self, path):
        r = await self.call("Page.captureScreenshot", {"format": "png"})
        open(path, "wb").write(base64.b64decode(r["result"]["data"]))


WAIT = """(async () => {
  const t0 = performance.now();
  const started = document.getElementById('statusText').textContent;
  while (performance.now() - t0 < 240000) {
    const s = document.getElementById('statusText').textContent || '';
    if (s.includes('运行中')) return {ok: true, ms: Math.round(performance.now()-t0), status: s};
    await new Promise(r => setTimeout(r, 300));
  }
  return {ok: false, status: document.getElementById('statusText').textContent};
})()"""

STATE = """(() => ({
  status: document.getElementById('statusText').textContent,
  cards: document.querySelectorAll('.card').length,
  gen: document.getElementById('btnGenerate').disabled,
  toast: document.getElementById('toast').hidden ? '' : document.getElementById('toast').textContent,
  mode: document.querySelector('.tab.active').dataset.mode,
  refs: document.querySelectorAll('.thumb').length
}))()"""


async def wait_gen(c, limit=300):
    t0 = time.time()
    while time.time() - t0 < limit:
        st = await c.eval(STATE)
        if not st["gen"]:
            return st
        await asyncio.sleep(2)
    return await c.eval(STATE)


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        await c.call("Runtime.enable")
        print("initial:", await c.eval(STATE), flush=True)

        # stop (should be managed from the previous session? fresh app: not started)
        await c.eval("document.getElementById('btnStop').click()")
        await asyncio.sleep(1.5)
        print("after stop:", await c.eval(STATE), flush=True)

        # start
        await c.eval("document.getElementById('btnStart').click()")
        r = await c.eval(WAIT)
        print("start:", r, flush=True)

        # generate
        prompt = "雨夜便利店门口，一只橘猫蹲在纸箱上看着街面的倒影，霓虹被雨水拉长，电影感，35mm"
        await c.eval(f"""(() => {{
          const el = document.getElementById('prompt');
          el.value = {json.dumps(prompt)};
          el.dispatchEvent(new Event('input', {{bubbles:true}}));
          document.getElementById('seed').value = 2026;
          document.getElementById('btnGenerate').click();
        }})()""")
        st = await wait_gen(c)
        print("t2i:", st, flush=True)

        # use as reference -> edit
        await c.eval("document.querySelector('.card img').click()")
        await asyncio.sleep(0.4)
        await c.eval("document.getElementById('lightboxRef').click()")
        await asyncio.sleep(0.8)
        await c.eval(f"""(() => {{
          const el = document.getElementById('prompt');
          el.value = {json.dumps("改成水墨画风格，保留猫、纸箱和便利店，黑白为主略带暖色")};
          el.dispatchEvent(new Event('input', {{bubbles:true}}));
          document.getElementById('btnGenerate').click();
        }})()""")
        st = await wait_gen(c)
        print("edit:", st, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_final.png")


asyncio.run(main())
