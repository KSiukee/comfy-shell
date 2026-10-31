"""Crop annotation smoke test: open crop modal, drag a box, confirm, train 2 steps."""
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
                const r = await fetch(im.src);
                const b = await r.blob();
                files.push(new File([b], 'crop_' + files.length + '.png', {type: b.type}));
            }
            window.__cropFiles = files;
            return files.length;
        })()""")
        await c.eval("document.getElementById('libTrain').click()")
        await asyncio.sleep(0.5)
        await c.eval("""(() => {
            trainState.files = window.__cropFiles;
            trainState.src = 'folder';
            document.querySelector('input[name="trainSrc"][value="folder"]').checked = true;
            document.getElementById('trainCaptionRow').hidden = false;
            trainRefreshSource();
            return trainAnn.length;
        })()""")
        await c.eval("document.querySelectorAll('#annList button')[0].click()")
        await asyncio.sleep(1.0)
        drag = await c.eval("""(() => {
            const stage = document.getElementById('cropStage');
            const img = document.getElementById('cropImg');
            const r = img.getBoundingClientRect();
            const mk = (t, x, y) => new PointerEvent(t, {clientX: r.left + r.width * x, clientY: r.top + r.height * y, bubbles: true, pointerId: 7});
            stage.dispatchEvent(mk('pointerdown', 0.2, 0.15));
            stage.dispatchEvent(mk('pointermove', 0.72, 0.85));
            stage.dispatchEvent(mk('pointerup', 0.72, 0.85));
            return {open: !document.getElementById('cropModal').hidden, box: document.getElementById('cropBox').hidden};
        })()""")
        print("crop drag:", drag, flush=True)
        await c.eval("document.getElementById('cropOk').click()")
        ann = await c.eval("({crop: trainAnn[0].crop, btn: trainAnn[0].cropBtn.textContent})")
        print("annotation:", ann, flush=True)

        await c.eval("""(() => {
            document.getElementById('trainName').value = 'crop_probe';
            document.getElementById('trainSteps').value = 2;
            document.getElementById('trainRank').value = 8;
            document.getElementById('trainSize').value = '384';
            document.querySelector('#trainModal details').open = true;
            document.getElementById('trainEval').value = 0;
            document.getElementById('trainGo').click();
        })()""")
        t0 = time.time()
        for _ in range(300):
            await asyncio.sleep(4)
            if not (await c.eval("document.getElementById('trainGo').textContent")).startswith("训练中"):
                break
        print(f"train {time.time()-t0:.0f}s:", await c.eval("document.getElementById('trainProgressText').textContent"), flush=True)


asyncio.run(main())
