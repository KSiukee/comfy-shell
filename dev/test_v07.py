"""v0.7: train a tiny LoRA on the selected images (requires bf16 base weights)."""
import asyncio, json, time, urllib.request
import websockets

def ws_url():
    d = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json", timeout=5).read())
    return [p for p in d if p.get("type") == "page"][0]["webSocketDebuggerUrl"]

async def main():
    async with websockets.connect(ws_url(), max_size=64*1024*1024) as ws:
        i = 0
        async def ev(expr, timeout=3600):
            nonlocal i
            i += 1
            await ws.send(json.dumps({"id": i, "method": "Runtime.evaluate",
                                      "params": {"expression": expr, "awaitPromise": True, "returnByValue": True}}))
            t0 = time.time()
            while time.time() - t0 < timeout:
                m = json.loads(await ws.recv())
                if m.get("id") == i:
                    r = m.get("result", {})
                    if "exceptionDetails" in r:
                        raise RuntimeError(str(r["exceptionDetails"])[:600])
                    return r.get("result", {}).get("value")
            raise TimeoutError("eval timeout")

        imgs = await ev("window.__TAURI__.core.invoke('library_scan').then(l=>l.slice(0,2).map(x=>x.path))")
        print("images:", imgs, flush=True)
        t0 = time.time()
        res = await ev(f"""window.__TAURI__.core.invoke('train_lora', {{req: {{
            name: 'probe_v1', steps: 32, rank: 8, lr: 0.0002,
            images: {json.dumps(imgs)}, captions: ['test style sample', 'test style sample']}}}})
            .then(r => 'OK ' + r).catch(e => 'ERR ' + e)""")
        print(f"train ({time.time()-t0:.0f}s):", res, flush=True)

asyncio.run(main())
