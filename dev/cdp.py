"""Drive the ComfyShell WebView2 UI through the Chrome DevTools Protocol.

Usage: python cdp.py <command> [args]
  eval "<js>"            evaluate JS, print the JSON result
  shot <path.png>        capture a full-window screenshot
  type <id> <text>       set an input value (fires input event)
  click <id>             click an element by id
"""
import asyncio
import base64
import json
import sys
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
        r = await self.call("Runtime.evaluate", {
            "expression": expr, "awaitPromise": True, "returnByValue": True})
        res = r.get("result", {})
        if "exceptionDetails" in res:
            return {"__error": res["exceptionDetails"].get("text", "js error"),
                    "detail": str(res["exceptionDetails"])[:500]}
        return res.get("result", {}).get("value")


async def main():
    cmd = sys.argv[1]
    async with websockets.connect(page_ws(), max_size=64 * 1024 * 1024) as ws:
        c = CDP(ws)
        await c.call("Runtime.enable")
        if cmd == "eval":
            print(json.dumps(await c.eval(sys.argv[2]), ensure_ascii=False))
        elif cmd == "shot":
            r = await c.call("Page.captureScreenshot", {"format": "png"})
            data = base64.b64decode(r["result"]["data"])
            open(sys.argv[2], "wb").write(data)
            print("saved", sys.argv[2], len(data))
        elif cmd == "type":
            eid, text = sys.argv[2], sys.argv[3]
            expr = f"""(() => {{
              const el = document.getElementById({json.dumps(eid)});
              if (!el) return 'missing';
              el.value = {json.dumps(text)};
              el.dispatchEvent(new Event('input', {{bubbles:true}}));
              el.dispatchEvent(new Event('change', {{bubbles:true}}));
              return 'ok';
            }})()"""
            print(json.dumps(await c.eval(expr), ensure_ascii=False))
        elif cmd == "click":
            expr = f"""(() => {{
              const el = document.getElementById({json.dumps(sys.argv[2])});
              if (!el) return 'missing';
              el.click();
              return 'ok';
            }})()"""
            print(json.dumps(await c.eval(expr), ensure_ascii=False))


asyncio.run(main())
