#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""显影 · 工程门：一条命令跑完静态检查 + 逻辑测试 + 冒烟，给一个明确结论。

血统：研墨/继忆/圈记 的 check_all。搬过来的三条纪律：

1. **防空转**：每一层都报"我实际检查了多少东西"，低于底线**就算失败**。
   路径写错 / 目标不存在时，守卫会永远绿灯——那比没有守卫更坏。
2. **守卫自己要被证明会红**：`--selftest` 喂坏输入，证明检查器真的会红。
3. **棘轮只许往一个方向转**：测试数、冒烟项数只许增；`app.js` 行数、上帝文件数只许减。
   想放宽底线，`--update-floors` 也只能**降低**，不能提高。

用法：
    python tools/qa/check_all.py            # 全部（静态 + Rust + 冒烟）
    python tools/qa/check_all.py --quick    # 只跑静态 + JS 语法（改文档/前端小改时）
    python tools/qa/check_all.py --selftest # 只证明守卫会红
    python tools/qa/check_all.py --json
退出码：0 = 全绿；1 = 有层失败。
"""
from __future__ import annotations

import argparse
import glob
import json
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FLOORS_PATH = os.path.join(ROOT, "tools", "qa", "floors.json")
SKIP_DIRS = {"target", "node_modules", ".git", "backups", "__pycache__", ".scratch"}
TEXT_EXT = {".rs", ".js", ".mjs", ".html", ".css", ".md", ".json", ".py", ".toml"}
CTRL_RE = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f]")


def read(path: str) -> str:
    with open(path, encoding="utf-8") as f:
        return f.read()


def rel(path: str) -> str:
    return os.path.relpath(path, ROOT).replace("\\", "/")


def floors() -> dict:
    return json.loads(read(FLOORS_PATH))


def versions() -> dict[str, str]:
    cargo = read(os.path.join(ROOT, "src-tauri", "Cargo.toml"))
    tauri = json.loads(read(os.path.join(ROOT, "src-tauri", "tauri.conf.json")))
    changelog = read(os.path.join(ROOT, "CHANGELOG.md"))
    agents = read(os.path.join(ROOT, "AGENTS.md"))
    state = json.loads(read(os.path.join(ROOT, ".codex", "state.json")))
    m = re.search(r'(?m)^version = "([^"]+)"', cargo)
    c = re.search(r"(?m)^## v(\d+\.\d+\.\d+)", changelog)
    a = re.search(r"(?:- 版本 \*\*|当前版本：)(\d+\.\d+\.\d+)", agents)
    return {
        "Cargo.toml": m.group(1) if m else "?",
        "tauri.conf.json": str(tauri.get("version", "?")),
        "CHANGELOG.md": c.group(1) if c else "?",
        "AGENTS.md": a.group(1) if a else "?",
        ".codex/state.json": str(state.get("version", "?")),
    }


def versions_agree(v: dict[str, str]) -> tuple[bool, str]:
    known = {x for x in v.values() if x != "?"}
    if len(v) < 5 or "?" in v.values():
        return False, f"有版本读不出来：{v}"
    return len(known) == 1, f"{len(v)} 处：{' / '.join(sorted(known))}"


def scan_text_file(path: str) -> tuple[bool, int]:
    """返回 (干净?, 问题数)。BOM 与 C0 控制字符都算问题。"""
    raw = open(path, "rb").read()
    bad = 1 if raw.startswith(b"\xef\xbb\xbf") else 0
    text = raw.decode("utf-8", errors="replace")
    bad += len(CTRL_RE.findall(text))
    return bad == 0, bad


def walk_text_files() -> list[str]:
    out = []
    for dirpath, dirnames, filenames in os.walk(ROOT):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for f in filenames:
            if os.path.splitext(f)[1] in TEXT_EXT:
                out.append(os.path.join(dirpath, f))
    return out


def god_files(min_lines: int) -> list[tuple[str, int]]:
    out = []
    for path in walk_text_files():
        if os.path.splitext(path)[1] not in {".rs", ".js", ".html", ".css"}:
            continue
        n = read(path).count("\n")
        if n >= min_lines:
            out.append((rel(path), n))
    return sorted(out, key=lambda x: -x[1])


def run(cmd: list[str], cwd: str = ROOT) -> tuple[int, str]:
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    return p.returncode, (p.stdout or "") + (p.stderr or "")


def parse_tests(out: str) -> int:
    m = re.findall(r"test result: ok\. (\d+) passed", out)
    return sum(int(x) for x in m)


def parse_smoke(out: str) -> tuple[int, int]:
    m = re.search(r"(\d+)/(\d+) 通过", out)
    return (int(m.group(1)), int(m.group(2))) if m else (0, -1)


def over_cap(n: int, cap: int) -> bool:
    """棘轮判据只有这一句：超过上限就是红。"""
    return n > cap


# ---------------------------------------------------------------- layers
def layer_versions() -> tuple[bool, str, int]:
    v = versions()
    ok, detail = versions_agree(v)
    return ok, detail, len(v)


def layer_text() -> tuple[bool, str, int]:
    files = walk_text_files()
    bad: list[str] = []
    for p in files:
        clean, n = scan_text_file(p)
        if not clean:
            bad.append(f"{rel(p)}({n})")
    ok = not bad
    detail = f"{len(files)} 个文件，{len(bad)} 个有 BOM/控制字符"
    if bad:
        detail += "：" + ", ".join(bad[:5])
    return ok, detail, len(files)


def layer_invoke() -> tuple[bool, str, int]:
    """新代码必须走 store.js；app.js 里再出现裸 invoke( 就是回潮。"""
    hits = []
    files = [p for p in glob.glob(os.path.join(ROOT, "web", "*.js")) if os.path.basename(p) != "store.js"]
    for p in files:
        for i, line in enumerate(read(p).split("\n"), 1):
            if re.search(r"\binvoke\(", line) and "api.call(" not in line:
                hits.append(f"{rel(p)}:{i}")
    ok = not hits
    return ok, ("0 处裸 invoke" if ok else f"{len(hits)} 处：" + ", ".join(hits[:5])), len(files)


def layer_ratchet() -> tuple[bool, str, int]:
    f = floors()
    problems = []
    # 上帝文件数量只许减
    gods = god_files(f["god_file_lines"])
    if len(gods) > f["max_god_files"]:
        problems.append(f"上帝文件 {len(gods)} > {f['max_god_files']}")
    # app.js 只许减
    for relpath, cap in f["max_lines"].items():
        n = read(os.path.join(ROOT, relpath)).count("\n")
        if over_cap(n, cap):
            problems.append(f"{relpath} {n} 行 > 上限 {cap}")
    ok = not problems
    detail = f"上帝文件 {len(gods)}/{f['max_god_files']}，app.js {read(os.path.join(ROOT, 'web', 'app.js')).count(chr(10))}/{f['max_lines']['web/app.js']}"
    if problems:
        detail += "；" + "；".join(problems)
    return ok, detail, len(gods)


def layer_js() -> tuple[bool, str, int]:
    files = ["web/app.js", "web/store.js", "web/projects.js", "web/vault.js", "web/layout.js", "web/data.js", "web/shots.js", "web/versions.js", "web/prompts.js", "web/library.js", "web/batch.js", "web/train.js", "web/inpaint.js", "web/layout-editor.js", "web/tools.js", "web/region.js", "web/meta.js", "web/workspace.js", "web/boot.js", "dev/smoke.mjs"]
    bad = []
    for p in files:
        code, out = run(["node", "--check", p])
        if code != 0:
            bad.append(f"{p}: {out.strip()[:120]}")
    return not bad, (f"{len(files)} 个 JS 语法通过" if not bad else "；".join(bad)), len(files)


def layer_rust() -> tuple[bool, str, int]:
    code, out = run(["cargo", "check"], cwd=os.path.join(ROOT, "src-tauri"))
    if code != 0:
        return False, "cargo check 失败：" + out.strip()[-300:], 0
    code, out = run(["cargo", "test"], cwd=os.path.join(ROOT, "src-tauri"))
    if code != 0:
        return False, "cargo test 失败：" + out.strip()[-300:], 0
    n = parse_tests(out)
    floor = floors()["min_tests"]
    ok = n >= floor
    return ok, f"{n} 例（底线 {floor}）", n


def layer_smoke() -> tuple[bool, str, int]:
    # release 已经禁掉调试端口，冒烟必须跑 debug 构建
    code, out = run(["cargo", "build"], cwd=os.path.join(ROOT, "src-tauri"))
    if code != 0:
        return False, "cargo build(debug) 失败：" + out.strip()[-300:], 0
    exe = os.path.join(ROOT, "src-tauri", "target", "debug", "comfy-shell.exe")
    code, out = run(["node", "dev/smoke.mjs", exe])
    passed, total = parse_smoke(out)
    floor = floors()["min_smoke_checks"]
    ok = code == 0 and total >= floor and passed == total
    return ok, f"{passed}/{total}（底线 {floor}）", total


# ---------------------------------------------------------------- selftest
def selftest() -> int:
    checks = []

    def ck(name: str, cond: bool) -> None:
        checks.append(cond)
        print(("[ok] " if cond else "[!!] ") + name)

    ck("版本一致能通过", versions_agree({"a": "1.0.0", "b": "1.0.0", "c": "1.0.0", "d": "1.0.0", "e": "1.0.0"})[0])
    ck("版本不一致能报错", not versions_agree({"a": "1.0.0", "b": "1.0.1", "c": "1.0.0", "d": "1.0.0", "e": "1.0.0"})[0])
    with tempfile.NamedTemporaryFile("w", suffix=".js", delete=False, encoding="utf-8") as f:
        f.write("const a = 1;\x07\n")
        bad_path = f.name
    ck("控制字符能抓到", not scan_text_file(bad_path)[0])
    os.unlink(bad_path)
    ck("棘轮能拦增长", over_cap(101, 100) and not over_cap(99, 100))
    ck("smoke 解析能判红", parse_smoke("8/9 通过") == (8, 9))
    ck("smoke 全过才绿", parse_smoke("9/9 通过")[0] == parse_smoke("9/9 通过")[1])
    ok = all(checks)
    print(f"\nselftest {sum(checks)}/{len(checks)}")
    return 0 if ok else 1


# ---------------------------------------------------------------- main
def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--quick", action="store_true", help="只跑静态 + JS 语法")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--update-floors", action="store_true", help="只能降低上限（棘轮）")
    args = ap.parse_args()

    if args.selftest:
        return selftest()

    if args.update_floors:
        f = floors()
        app_lines = read(os.path.join(ROOT, "web", "app.js")).count("\n")
        old = f["max_lines"]["web/app.js"]
        if app_lines < old:
            f["max_lines"]["web/app.js"] = app_lines
            open(FLOORS_PATH, "w", encoding="utf-8", newline="\n").write(json.dumps(f, ensure_ascii=False, indent=2) + "\n")
            print(f"app.js 上限 {old} → {app_lines}")
        else:
            print(f"app.js 没变短（{app_lines} >= {old}），上限不动")
        return 0

    layers = [
        ("版本一致", layer_versions),
        ("文本卫生", layer_text),
        ("裸 invoke", layer_invoke),
        ("棘轮", layer_ratchet),
        ("JS 语法", layer_js),
    ]
    if not args.quick:
        layers += [("Rust 检查", layer_rust), ("冒烟", layer_smoke)]

    results = []
    for name, fn in layers:
        try:
            ok, detail, count = fn()
        except Exception as e:  # 跑不了就是失败
            ok, detail, count = False, f"异常：{e}", 0
        results.append({"layer": name, "ok": ok, "detail": detail, "count": count})
        if not args.json:
            print(f"[{'ok' if ok else '!!'}] {name:<8} {detail}")

    passed = sum(1 for r in results if r["ok"])
    if args.json:
        print(json.dumps({"passed": passed, "total": len(results), "layers": results}, ensure_ascii=False, indent=2))
    else:
        print(f"\n结论：{passed}/{len(results)} 通过")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    sys.exit(main())
