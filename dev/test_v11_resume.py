"""v0.11: resume training from an existing LoRA (8 steps) and confirm the new file loads."""
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
        await c.eval("document.getElementById('btnLibrary').click()")
        for _ in range(30):
            await asyncio.sleep(1)
            if await c.eval("document.querySelectorAll('.lib-item').length") > 4:
                break
        await c.eval("""(() => {
            const b=[...document.querySelectorAll('.lib-item input[type=checkbox]')];
            b[0].click(); b[1].click();
        })()""")
        await asyncio.sleep(0.4)
        await c.eval("document.getElementById('libTrain').click()")
        await asyncio.sleep(0.5)
        opts = await c.eval("([...document.getElementById('trainExisting').options].map(o=>o.value).filter(v=>v.includes('folder_probe')))")
        print("resume options:", opts, flush=True)
        target = opts[0] if opts else ""
        await c.eval(f"""(() => {{
            const ex=document.getElementById('trainExisting');
            ex.value={json.dumps(target)};
            ex.dispatchEvent(new Event('change'));
            document.getElementById('trainSteps').value=8;
            document.getElementById('trainRank').value=8;
            document.getElementById('trainCaption').value='';
            document.getElementById('trainGo').click();
        }})()""")
        t0 = time.time()
        last = None
        for _ in range(400):
            await asyncio.sleep(4)
            st = await c.eval("""({btn:document.getElementById('trainGo').textContent,
                bar:document.getElementById('trainBar').style.width,
                txt:document.getElementById('trainProgressText').textContent,
                loss:document.getElementById('trainResult').hidden ? '' : 'shown',
                lora:document.getElementById('lora').value,
                toast:document.getElementById('toast').hidden ? '' : document.getElementById('toast').textContent,
                name:document.getElementById('trainName').value})""")
            if st != last:
                print(f"  {time.time()-t0:5.0f}s {st}", flush=True)
                last = st
            if not st["btn"].startswith("训练中"):
                break
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_resume.png")

        # use the resumed LoRA for one generation to prove it loads
        await c.eval("""(() => {
            document.getElementById('trainCancel').click();
            document.getElementById('btnLibrary').click();
        })()""")
        await asyncio.sleep(1)
        await c.eval("""(() => {
            document.getElementById('model').value='flux3';
            document.getElementById('prompt').value='a quiet street market at dawn';
            document.getElementById('steps').value=8;
            document.getElementById('width').value=768; document.getElementById('height').value=512;
            document.getElementById('btnGenerate').click();
        })()""")
        await asyncio.sleep(35)
        print("gen:", await c.eval("({cards:document.querySelectorAll('.card').length, toast:document.getElementById('toast').hidden?'':document.getElementById('toast').textContent})"), flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\shot_resume_gen.png")


asyncio.run(main())
