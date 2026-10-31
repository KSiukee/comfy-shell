import asyncio, json, urllib.request, base64
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

    async def eval(self, expr, timeout=300):
        r = await asyncio.wait_for(self.call("Runtime.evaluate", {"expression": expr, "awaitPromise": True, "returnByValue": True}), timeout)
        res = r.get("result", {})
        if "exceptionDetails" in res:
            raise RuntimeError(str(res["exceptionDetails"])[:600])
        return res.get("result", {}).get("value")

    async def shot(self, path):
        r = await self.call("Page.captureScreenshot", {"format": "png"})
        open(path, "wb").write(base64.b64decode(r["result"]["data"]))


async def main():
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        info = await c.eval("""(() => {
            document.getElementById('model').value='moss';
            document.getElementById('model').dispatchEvent(new Event('change'));
            const loras=[...document.getElementById('lora').options].map(o=>o.value).filter(v=>v);
            return {
              presetVisible: !document.getElementById('btnMossPreset').hidden,
              tier: document.getElementById('slgTier').value,
              slg: document.getElementById('mossSlg').checked,
              loraCount: loras.length,
              mossLoras: loras.filter(v=>/moss/i.test(v))
            };
        })()""")
        print("ui:", info, flush=True)

        await c.eval("""(() => {
            document.getElementById('btnMossPreset').click();
            return {w:document.getElementById('width').value, s:document.getElementById('steps').value, cfg:document.getElementById('cfg').value};
        })()""")
        print("preset:", await c.eval("({w:document.getElementById('width').value, s:document.getElementById('steps').value, cfg:document.getElementById('cfg').value})"), flush=True)

        # beauty LoRA at 8 steps 512 with SLG
        await c.eval("""(() => {
            document.getElementById('width').value=512; document.getElementById('height').value=512;
            document.getElementById('steps').value=8; document.getElementById('prompt').value='portrait of a woman, natural skin texture, soft window light';
            const l=document.getElementById('lora');
            l.value=[...l.options].map(o=>o.value).find(v=>/beauty/i.test(v))||'';
            l.dispatchEvent(new Event('change'));
            document.getElementById('loraStrength').value=0.7;
            document.getElementById('btnGenerate').click();
            return l.value;
        })()""")
        for _ in range(90):
            await asyncio.sleep(2)
            if not await c.eval("document.getElementById('btnGenerate').disabled"):
                break
        print("beauty gen:", await c.eval("({cards:document.querySelectorAll('.card').length, toast:document.getElementById('toast').hidden?'':document.getElementById('toast').textContent})"), flush=True)

        # strong tier
        await c.eval("""(() => {
            document.getElementById('slgTier').value='22,24,26';
            document.getElementById('steps').value=6;
            const l=document.getElementById('lora'); l.value=''; l.dispatchEvent(new Event('change'));
            document.getElementById('btnGenerate').click(); return 'go';
        })()""")
        for _ in range(90):
            await asyncio.sleep(2)
            if not await c.eval("document.getElementById('btnGenerate').disabled"):
                break
        print("strong tier:", await c.eval("({toast:document.getElementById('toast').hidden?'':document.getElementById('toast').textContent})"), flush=True)
        await c.shot(r"D:\deepseek-files\ai-tools\comfy-shell\screenshot.png")


asyncio.run(main())
