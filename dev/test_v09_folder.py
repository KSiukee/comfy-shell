"""v0.9: train from an external folder (auto crop+caption), with progress bar and loss curve."""
import asyncio
import base64
import json
import time
import urllib.request

import websockets


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

    async def eval(self, expr, timeout=3600):
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
        print("boot:", await c.eval("({status:document.getElementById('statusText').textContent})"), flush=True)

        await c.eval("document.getElementById('btnLibrary').click()")
        for _ in range(30):
            await asyncio.sleep(1)
            if await c.eval("document.querySelectorAll('.lib-item').length") > 4:
                break

        # simulate a picked external folder: build File objects from library images via fetch(dataurl)
        files_js = """(async () => {
            const imgs = [...document.querySelectorAll('.lib-item img')].slice(0, 4);
            const files = [];
            for (const im of imgs) {
                const r = await fetch(im.src);
                const b = await r.blob();
                files.push(new File([b], 'ext_' + files.length + '.png', {type: b.type}));
            }
            trainState.files = files;
            trainState.src = 'folder';
            document.querySelector('input[name="trainSrc"][value="folder"]').checked = true;
            document.getElementById('trainCaptionRow').hidden = false;
            trainRefreshSource();
            return files.length;
        })()"""
        await c.eval("document.getElementById('libTrain').click()")
        await asyncio.sleep(0.5)
        n = await c.eval(files_js)
        print("folder files:", n, flush=True)


        state = await c.eval("({count:document.getElementById('trainCount').textContent, modal:!document.getElementById('trainModal').hidden})")
        print("modal:", state, flush=True)

        await c.eval("""(() => {
            document.getElementById('trainName').value = 'folder_probe_6';
            document.getElementById('trainSteps').value = 16;
            document.getElementById('trainRank').value = 8;
            document.getElementById('trainCaption').value = 'a quiet street market, warm light';
            document.getElementById('trainGo').click();
        })()""")

        await asyncio.sleep(35)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_train_progress.png")
        t0 = time.time()
        last = None
        for _ in range(400):
            await asyncio.sleep(4)
            st = await c.eval("""({btn:document.getElementById('trainGo').textContent,
                bar:document.getElementById('trainBar').style.width,
                txt:document.getElementById('trainProgressText').textContent,
                loss:document.getElementById('trainResult').hidden ? '' : 'shown',
                lora:document.getElementById('lora').value})""")
            if st != last:
                print(f"  {time.time()-t0:5.0f}s {st}", flush=True)
                last = st
            if not st["btn"].startswith("训练中"):
                break
        print(f"done ({time.time()-t0:.0f}s):", last, flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_train_folder.png")


asyncio.run(main())






