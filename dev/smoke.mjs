// 冒烟测试：起壳 → 关键 UI / 后端命令在 → 关壳。约 10 秒，不生成图片。
// 用法：node dev/smoke.mjs [exe 路径]
// 需要 Node >= 22（全局 WebSocket）。会占用 9223 调试端口。
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const port = 9223;
const exe = process.argv[2] || fileURLToPath(new URL("../src-tauri/target/release/comfy-shell.exe", import.meta.url));
if (!existsSync(exe)) {
  console.error("找不到 exe:", exe, "\n先构建：cd src-tauri; cargo tauri build --no-bundle");
  process.exit(2);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const child = spawn(exe, [], {
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
  stdio: "ignore",
  windowsHide: false,
});
let childExited = false;
child.on("exit", () => {
  childExited = true;
});

let ws;
let nextId = 0;
const pending = new Map();

async function pageTarget(timeoutMs = 25000) {
  const t0 = Date.now();
  while (Date.now() - t0 < timeoutMs) {
    if (childExited) {
      throw new Error("应用启动后立刻退出：可能已有显影在跑（单实例）。先关掉它再跑 smoke。");
    }
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = list.find((t) => t.type === "page");
      if (page) return page;
    } catch {}
    await sleep(400);
  }
  throw new Error("CDP 页面没起来（应用启动失败？）");
}

function call(method, params = {}) {
  const id = ++nextId;
  ws.send(JSON.stringify({ id, method, params }));
  return new Promise((resolve) => pending.set(id, resolve));
}

async function evaluate(expression) {
  const r = await call("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  const res = r.result ?? {};
  if (res.exceptionDetails) {
    throw new Error(res.exceptionDetails.exception?.description || res.exceptionDetails.text || "JS 异常");
  }
  return res.result?.value;
}

const checks = [];
function check(name, ok, detail = "") {
  checks.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? "  — " + detail : ""}`);
}

try {
  const page = await pageTarget();
  ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve);
    ws.addEventListener("error", reject);
  });
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  });
  await call("Runtime.enable");
  await sleep(1500); // boot(): 设置/模型列表/图库首屏

  check("窗口标题是「显影」", (await evaluate("document.title")) === "显影");
  check("store.js 已加载", (await evaluate("typeof api?.call === 'function'")) === true);
  check("主输入框在", (await evaluate("!!document.getElementById('prompt')")) === true);
  check("去AI感按钮在", (await evaluate("!!document.getElementById('btnDeAi')")) === true);
  check("排版模板下拉在", (await evaluate("!!document.getElementById('layoutPreset')")) === true);
  check("projects.js 已加载", (await evaluate("typeof window.projectUI === 'object'")) === true);
  check("分隔条在", (await evaluate("!!document.getElementById('panelSplitter')")) === true);
  const projects = await evaluate("api.call('project_list', { includeHidden: true })");
  check("project_list 返回数组", Array.isArray(projects), `${Array.isArray(projects) ? projects.length : "?"} 个项目`);

  const settings = await evaluate("api.call('settings_get')");
  check("settings_get 返回对象", !!settings && typeof settings === "object", settings?.comfyRoot || "");
  check("设置里有 schemaVersion", Number(settings?.schemaVersion) >= 1, String(settings?.schemaVersion));

  const tpls = await evaluate("api.call('layout_tpl_list')");
  check("layout_tpl_list 返回数组", Array.isArray(tpls), `${Array.isArray(tpls) ? tpls.length : "?"} 个`);

  const hist = await evaluate("api.call('history_list')");
  check("history_list 返回数组", Array.isArray(hist));
} catch (e) {
  check("启动/连接", false, String(e.message || e));
} finally {
  try {
    ws?.close();
  } catch {}
  child.kill();
  await sleep(800);
  const failed = checks.filter((c) => !c.ok);
  console.log(`\n${checks.length - failed.length}/${checks.length} 通过`);
  process.exit(failed.length ? 1 : 0);
}
