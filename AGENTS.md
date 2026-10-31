# comfy-shell（ComfyShell）

本地生图/改图壳：不打开 ComfyUI 界面，Rust(Tauri 2) 后端 + 纯静态前端，管模型、训练、图库、排版、姿势库。

## 当前状态
- 版本 **0.37.0**（2026-10-31）
- 可执行：`src-tauri\target\release\comfy-shell.exe`
- ComfyUI：`D:\deepseek-files\ai-tools\ComfyUI_windows_portable`（v0.42.1，端口 8188）
- 训练外挂：`ai-tools\sdvenv`（kohya sd-scripts，MOSS 专用）

## 接手先读
1. `docs/功能清单.md`：现在有什么
2. `docs/架构.md`：代码在哪、数据在哪、关键决策
3. `CHANGELOG.md`：怎么走到今天的（v0.1–v0.12 为重建，标注推断）
4. `docs/待办.md`：下一步
5. `docs/踩坑记录.md`：45 条实测坑与经验（含待验/可疑标记）

## 铁律
- 改前端后 `node --check web/app.js`
- 构建：`cd src-tauri; $env:HTTP_PROXY=...; cargo tauri build --no-bundle`
- 测试走 CDP：`dev/cdp.py`（需 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`）
- 训练必须壳托管 ComfyUI；训练前勾「重启清显存」
- 不要给 MOSS 外挂 ModelSamplingSD3（会出噪点）；SLG 用 SkipLayerGuidanceDiT
- layout JSON 只对 MOSS；Qwen 喂 JSON 会更差

## 任务
见 `docs/待办.md`；交接系统任务条目待注册（`.codex/state.json` 已建骨架）。
