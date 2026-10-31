"""Qwen adult-proportion check: cfg 4 + negative vs cfg 1, same seed."""
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
            raise RuntimeError(str(res["exceptionDetails"])[:700])
        return res.get("result", {}).get("value")


async def gen(c, cfg, tag):
    await c.eval(f"""(() => {{
        document.getElementById('model').value = 'qwen21';
        document.getElementById('model').dispatchEvent(new Event('change'));
        document.getElementById('prompt').value =
          'full body photo of a mature adult woman, 28 years old, tall, long legs, standing on a city street, fashion photography';
        document.getElementById('negative').value =
          'child, kid, toddler, loli, chibi, petite, short legs, bad anatomy, deformed legs, extra limbs';
        document.getElementById('steps').value = 30;
        document.getElementById('width').value = 832;
        document.getElementById('height').value = 1216;
        document.getElementById('seed').value = 42;
        document.getElementById('cfg').value = {cfg};
        document.getElementById('batch').value = 1;
        document.getElementById('btnGenerate').click();
        return 'go';
    }})()""")
    t0 = time.time()
    for _ in range(120):
        await asyncio.sleep(2)
        if not await c.eval("document.getElementById('btnGenerate').disabled"):
            break
    paths = await c.eval("window.__TAURI__.core.invoke('library_scan').then(l => l.slice(0,1).map(x=>x.path))")
    print(tag, f"{time.time()-t0:.0f}s", paths, flush=True)
    return paths[0] if paths else None


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        a = await gen(c, 4, "cfg4")
        b = await gen(c, 1, "cfg1")
        print(json.dumps({"cfg4": a, "cfg1": b}))


asyncio.run(main())
