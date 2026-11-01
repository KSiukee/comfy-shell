// CDP 小驱动（Node 版）：系统 Python 没装 websockets 时用这个，Node ≥22 自带 WebSocket。
//
// 用法（先起壳：WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 再运行 exe）：
//   node dev/cdp.mjs eval "document.title"
//   node dev/cdp.mjs shot logs/screenshots/x.png
//   node dev/cdp.mjs click shotsBtn
//   node dev/cdp.mjs type prompt "雨夜街头"
//
// 端口可用 CDP_PORT 覆盖，默认 9222。
const port = process.env.CDP_PORT || 9222;
const [cmd, ...rest] = process.argv.slice(2);

const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = list.find((t) => t.type === "page");
if (!page) {
  console.error("没找到页面（壳起来了吗？端口 " + port + "）");
  process.exit(2);
}
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((res, rej) => {
  ws.addEventListener("open", res);
  ws.addEventListener("error", rej);
});
let nextId = 0;
const pending = new Map();
ws.addEventListener("message", (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  }
});
function call(method, params = {}) {
  const id = ++nextId;
  ws.send(JSON.stringify({ id, method, params }));
  return new Promise((resolve) => pending.set(id, resolve));
}
async function evaluate(expression) {
  const r = await call("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  const res = r.result ?? {};
  if (res.exceptionDetails) {
    return { __error: res.exceptionDetails.exception?.description || res.exceptionDetails.text || "JS 异常" };
  }
  return res.result?.value;
}

await call("Runtime.enable");
let out;
if (cmd === "eval") {
  out = await evaluate(rest.join(" "));
} else if (cmd === "eval-file") {
  // 从文件读 JS：避免 PowerShell 把 $() 当子表达式展开（踩过一次，调了半天）
  const { readFileSync } = await import("node:fs");
  out = await evaluate(readFileSync(rest[0], "utf8"));
} else if (cmd === "shot") {
  const r = await call("Page.captureScreenshot", { format: "png" });
  const { writeFileSync, mkdirSync } = await import("node:fs");
  const { dirname } = await import("node:path");
  mkdirSync(dirname(rest[0]), { recursive: true });
  writeFileSync(rest[0], Buffer.from(r.result.data, "base64"));
  out = { saved: rest[0], bytes: r.result.data.length };
} else if (cmd === "click") {
  out = await evaluate(`(() => { const el = document.getElementById(${JSON.stringify(rest[0])}); if (!el) return "missing"; el.click(); return "ok"; })()`);
} else if (cmd === "type") {
  out = await evaluate(
    `(() => { const el = document.getElementById(${JSON.stringify(rest[0])}); if (!el) return "missing"; el.value = ${JSON.stringify(rest.slice(1).join(" "))}; el.dispatchEvent(new Event("input", {bubbles:true})); el.dispatchEvent(new Event("change", {bubbles:true})); return "ok"; })()`,
  );
} else {
  console.error("用法：eval <js> | shot <png> | click <id> | type <id> <text>");
  process.exit(2);
}
console.log(typeof out === "string" ? out : JSON.stringify(out, null, 2));
ws.close();
