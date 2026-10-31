"""v0.20: region marker (green keep / red erase) + annotation translation."""
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

    async def eval(self, expr, timeout=1800):
        r = await asyncio.wait_for(
            self.call("Runtime.evaluate", {"expression": expr, "awaitPromise": True, "returnByValue": True}),
            timeout,
        )
        res = r.get("result", {})
        if "exceptionDetails" in res:
            raise RuntimeError(str(res["exceptionDetails"])[:800])
        return res.get("result", {}).get("value")


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        await c.eval("document.getElementById('btnLibrary').click()")
        for _ in range(30):
            await asyncio.sleep(1)
            if await c.eval("document.querySelectorAll('.lib-item').length") > 4:
                break
        await c.eval("""(async () => {
            const imgs = [...document.querySelectorAll('.lib-item img')].slice(0, 2);
            const files = [];
            for (const im of imgs) {
                const r = await fetch(im.src); const b = await r.blob();
                files.push(new File([b], 'mark_' + files.length + '.png', {type: b.type}));
            }
            window.__mark = files;
            return files.length;
        })()""")
        await c.eval("document.getElementById('libTrain').click()")
        await asyncio.sleep(0.5)
        await c.eval("""(() => {
            trainState.files = window.__mark;
            trainState.src = 'folder';
            document.querySelector('input[name="trainSrc"][value="folder"]').checked = true;
            document.getElementById('trainCaptionRow').hidden = false;
            trainRefreshSource();
            document.querySelectorAll('#annList img')[0].click();
            return 'opened';
        })()""")
        await asyncio.sleep(1)
        drag = await c.eval("""(() => {
            const stage = document.getElementById('cropStage');
            const img = document.getElementById('cropImg');
            const r = img.getBoundingClientRect();
            const mk = (t, x, y) => new PointerEvent(t, {clientX: r.left + r.width * x, clientY: r.top + r.height * y, bubbles: true, pointerId: 9});
            // green keep
            stage.dispatchEvent(mk('pointerdown', 0.18, 0.1));
            stage.dispatchEvent(mk('pointermove', 0.8, 0.9));
            stage.dispatchEvent(mk('pointerup', 0.8, 0.9));
            // red erase
            document.getElementById('markErase').click();
            stage.dispatchEvent(mk('pointerdown', 0.05, 0.6));
            stage.dispatchEvent(mk('pointermove', 0.3, 0.95));
            stage.dispatchEvent(mk('pointerup', 0.3, 0.95));
            return {state: document.getElementById('markState').textContent};
        })()""")
        print("drag:", drag, flush=True)
        await c.eval("document.getElementById('cropOk').click()")
        ann = await c.eval("({crop: trainAnn[0].crop, erases: trainAnn[0].erases, btn: trainAnn[0].cropBtn.textContent})")
        print("annotation:", ann, flush=True)

        # translation
        await c.eval("""(() => {
            const rows = document.querySelectorAll('#annList .ann-row');
            rows[0].querySelectorAll('input')[0].value = '红色外套，银白长发，站在雨里';
            rows[0].querySelectorAll('input')[1].value = '背景杂物';
            return 'filled';
        })()""")
        await c.eval("document.getElementById('annTranslate').click()")
        t0 = time.time()
        for _ in range(30):
            await asyncio.sleep(2)
            txt = await c.eval("document.getElementById('annTranslate').textContent")
            if not txt.startswith("翻译中"):
                break
        print(f"translate ({time.time()-t0:.0f}s):", await c.eval("""(() => {
            const rows = document.querySelectorAll('#annList .ann-row');
            return [rows[0].querySelectorAll('input')[0].value, rows[0].querySelectorAll('input')[1].value];
        })()"""), flush=True)


asyncio.run(main())
