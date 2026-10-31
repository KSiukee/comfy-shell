# -*- coding: utf-8 -*-
import os, json, datetime

root = r"D:\deepseek-files\comfy-shell"
docs = os.path.join(root, "docs")
os.makedirs(docs, exist_ok=True)

changelog = """# 更新历史（重建版）

> 本文件在 v0.37.0 时补写。v0.1–v0.12 没有当时的发布记录，依据 `dev/test_*.py`、截图、README 旧版与代码痕迹重建，
> 标注「推断」的条目细节可能有偏差；v0.13 之后为当时实测记录。

## v0.37.0
- 本地小助手加工具：`pose`（搜姿势库）、`analyze`（分析当前参考图），最多 4 步。

## v0.36.0
- 「🤖 助手」：中文需求 → translate → expand → final，迷你工具循环（Qwen3.5-9B）。

## v0.35.0
- 排版框支持四角缩放；新增电商模板：主图 / 海报 / 左图右文 / 三分屏 / 四宫格。

## v0.34.0
- MOSS 排版模式：拖框 → layout JSON（abs_centralized / yxyx），实测标题+产品落位。
- 解析图支持 `extras` 动态分类（weather/time 等），可手动加分类。

## v0.33.0
- 两段式解析：VL 描述 → 文本模型拆九类；姿势不再混入场景，配饰明确 none。

## v0.32.0
- 「🔍 解析图」：本地 Qwen3-VL 拆 姿势/表情/衣着/配饰/背景/风格/色彩/光线/氛围，折叠原始 JSON。

## v0.31.0
- 横扫升级：按人像预设 / 按模型；5 档尺寸（512–1024）；每组 1/2/3 张。
- 实测 batch 2 同 seed 两张内容不同（像素均差 37）。

## v0.30.0
- 姿势库按实际文件过滤：288→280 组，缺机位自动回退，修复破图格子。

## v0.29.0
- 内置 Shisei 白模姿势库（280 组 × 5 机位，CC0）；点卡片加参考图，「填」写提示词。

## v0.28.0
- 人像预设：真实感 / 自然耐看 / 生图感 / 精致人像。

## v0.27.0
- 接入 MOSS-Imagine（真实感微调，INT8 5.2GB）为独立模型。

## v0.26.0
- Qwen 2.1 选 Anything2Real 自动切改图并填提示词。

## v0.25.0
- SLG 两档（22 / 22,24,26）+「官方预设」按钮（50 步 / CFG3 / 1024²）。

## v0.24.0
- 接通 MOSS SLG（SkipLayerGuidanceDiT skip 22），默认开，慢约 25%，暗部细节更实。

## v0.23.0
- 双 LoRA 槽；收藏落盘 `output\\_favorites`；图库缩略图大小滑条；步数/CFG 同行；MOSS 隐藏加速。

## v0.22.0
- 启动不阻塞 UI + 生成排队；图库缓存（0.2s）；训练引擎选择（内置/kohya）；MOSS 改 euler/50 步。

## v0.21.0
- kohya MOSS 训练验证通过（512 约 12.5 秒/步）；下载 MOSS Base；MOSS 默认 49 步 / CFG3。

## v0.20.0
- 标注器绿区保留 + 红区模糊；标注中→英翻译。

## v0.19.0
- 简单/专业双模式；训练傻瓜参数（角色/风格/物件/场景 × 轻/标准/重）。

## v0.18.0
- 标注框选裁切（crop）；训练前自动裁。

## v0.17.0
- LoRA 兼容表 + 强度滑条；按模型推荐/隐藏不兼容 LoRA。

## v0.16.0
- 训练显存旋钮（batch/grad/depth/offload/dtype）+ 预设 + 训练前重启清显存。

## v0.15.0
- SeedVR2 超分接入（3B）。

## v0.14.0
- 外部文件夹批量超分；批量队列。

## v0.13.0
- 扩写被拒自动回退本地 heretic（free_enhance）。

## v0.12.0
- 640 等比镜像填充（建筑类训练）。

## v0.11.0
- 续训（existing_lora）。

## v0.10.0
- 训练后固定 prompt 评测 + 实验 manifest。

## v0.9.0
- 外部文件夹训练 + 进度 + 损失曲线。

## v0.8.0（推断）
- 批量队列、模型横扫雏形。

## v0.7.0
- 训练 LoRA（内置 TrainLoraNode），首个可训练版本。

## v0.6.0
- 画笔蒙版 + 局部重绘（Qwen 2.1，羽化 + 参考图）。

## v0.5.0
- 中文搜索中→英；大图查看器；ESRGAN 超分。

## v0.4.0（推断）
- 图库搜索、导出对比图、批量改图、模型横扫。

## v0.3.0（推断）
- 图库、自定义工作流导入。

## v0.2.0（推断）
- 提示词扩写、历史记录。

## v0.1.0
- Tauri 壳 + 静默拉起 ComfyUI + 文生图 + 改图。
"""

features = """# 功能清单（v0.37.0）

## 生成
- 模型：Qwen-Image 2.1 / FLUX.3 Klein（蒸馏、Base）/ MOSS SFT / MOSS Base / MOSS-Imagine / 自定义 API 工作流
- 参考图：Qwen、FLUX 支持；最多 10 张（Qwen 1 张）
- 负向提示词中→英；扩写（官方 PE，被拒回退 heretic）
- 双 LoRA（同架构串联）；人像预设 4 套；MOSS 官方预设按钮
- 采样：MOSS 50 步 / CFG3 / euler / SLG（22 或 22,24,26）；Qwen 45 步 / CFG4；FLUX 蒸馏 16、Base 50

## 排版 / 辅助
- MOSS 排版模式：拖框 + 四角缩放 + 电商模板 → layout JSON
- 看图拆提示词：九类 + extras，两段式可选
- 白模姿势库：280 组 × 5 机位，参考图/描述/复制
- 本地小助手：translate / expand / pose / analyze，最多 4 步

## 资产
- 图库：缓存 + 分批渲染 + 缩略图滑条 + 收藏落盘 `_favorites`
- 搜索：文件名/提示词；中→英二次搜索
- 大图：缩放/翻页/4× 超分；右键删除/定位/当参考/复制提示词
- 超分：4x ESRGAN ×4 + SeedVR2 3B；批量与外部文件夹
- 重绘：画笔蒙版 + 羽化 + 洞内参考图

## 训练
- 引擎：内置（FLUX）/ kohya（MOSS、大分辨率）
- 逐图标注：要/不要/权重/绿区保留/红区模糊；自动 caption
- 续训、固定评测、实验 manifest、训练前重启清显存

## 剧情 / 批量
- 剧情拆分（本地 Qwen3.5）→ 人工确认 → 批量队列
- 横扫：按预设/按模型 × 5 档尺寸 × 1/2/3 张
"""

arch = """# 架构

## 组成
- `src-tauri/src/main.rs`：全部 Tauri 命令、训练编排、日志/实验记录
- `src-tauri/src/api.rs`：Profile 表、Graph 构建（生图/训练/重绘/超分）、提交与轮询
- `src-tauri/src/enhance.rs`：扩写、翻译、caption、剧情拆分、看图解析、两段式、迷你 agent
- `src-tauri/src/library.rs`：扫描/缓存/收藏/删除
- `src-tauri/src/history.rs`、`server.rs`：历史与 ComfyUI 进程管理
- `web/`：纯静态 HTML/CSS/JS（无构建），`window.__TAURI__` 调用
- `dev/`：CDP 自动化测试与探针脚本

## 数据
- 配置/历史/收藏/缓存：`%APPDATA%\\com.dsh.comfyshell\\`
- 姿势库：`ComfyUI\\input\\shisei\\`
- 输出：`ComfyUI\\output\\`（收藏镜像 `_favorites`，删除进 `_comfyshell_trash`）
- 外部训练：`ai-tools\\sdvenv` + `ai-tools\\sd-scripts`

## 关键决策
- 不打开 ComfyUI UI；进程由壳托管，训练前重启清显存
- MOSS layout JSON 自己拼（ComfyUI 节点 ARRAY 输入在 API 下校验不过）
- layout 只对 MOSS 生效；Qwen 不理解 bbox
- 图库缓存按目录 mtime + stars mtime 失效
- 冷启动不阻塞：auto-start 线程 + 异步状态 + ensureReady 排队
"""

todos = """# 待办（v0.37.0）

## 已确认要做
- [ ] Florence-2 检测/OCR：参考海报/菜单 → 自动生成排版框（估半天）
- [ ] 排版自定义模板保存（本地）
- [ ] 小助手接 generate / layout 工具（需限步）
- [ ] 连载：Series/Shot 持久化、设定卡、整话导出

## 已知问题
- MOSS 内置训练器只能 384；512 必须 kohya
- 训练 768 在 16G OOM
- 图库首次扫描几千张仍慢（仅缓存后续）
- 姿势库 47 组缺正面机位（已回退其它机位）
- 红区标注是模糊，不是掩码训练

## 技术债
- `main.rs` 单文件过大（>1500 行），命令可按域拆分
- 无自动化回归（只有 dev 脚本，需手动跑）
- README 与 docs 需随版本更新（v0.37 起补）
"""

agents = """# comfy-shell（ComfyShell）

本地生图/改图壳：不打开 ComfyUI 界面，Rust(Tauri 2) 后端 + 纯静态前端，管模型、训练、图库、排版、姿势库。

## 当前状态
- 版本 **0.37.0**（2026-10-31）
- 可执行：`src-tauri\\target\\release\\comfy-shell.exe`
- ComfyUI：`D:\\deepseek-files\\ai-tools\\ComfyUI_windows_portable`（v0.42.1，端口 8188）
- 训练外挂：`ai-tools\\sdvenv`（kohya sd-scripts，MOSS 专用）

## 接手先读
1. `docs/功能清单.md`：现在有什么
2. `docs/架构.md`：代码在哪、数据在哪、关键决策
3. `CHANGELOG.md`：怎么走到今天的（v0.1–v0.12 为重建，标注推断）
4. `docs/待办.md`：下一步

## 铁律
- 改前端后 `node --check web/app.js`
- 构建：`cd src-tauri; $env:HTTP_PROXY=...; cargo tauri build --no-bundle`
- 测试走 CDP：`dev/cdp.py`（需 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`）
- 训练必须壳托管 ComfyUI；训练前勾「重启清显存」
- 不要给 MOSS 外挂 ModelSamplingSD3（会出噪点）；SLG 用 SkipLayerGuidanceDiT
- layout JSON 只对 MOSS；Qwen 喂 JSON 会更差

## 任务
见 `docs/待办.md`；交接系统任务条目待注册（`.codex/state.json` 已建骨架）。
"""

for name, text in (("CHANGELOG.md", changelog), ("docs/功能清单.md", features),
                   ("docs/架构.md", arch), ("docs/待办.md", todos), ("AGENTS.md", agents)):
    path = os.path.join(root, name.replace("/", os.sep))
    os.makedirs(os.path.dirname(path), exist_ok=True)
    open(path, "w", encoding="utf-8", newline="\n").write(text)

state = {
    "version": "0.37.0",
    "project": "comfy-shell",
    "updated_at": datetime.datetime.now().strftime("%Y-%m-%d %H:%M"),
    "current_task": "v0.37.0：本地小助手加 pose/analyze 工具；文档、CHANGELOG、待办补齐。下一步候选：Florence-2 自动解析排版。",
    "tasks": [
        {"id": 1, "priority": 4, "difficulty": 4, "title": "Florence-2 检测/OCR：参考海报/菜单自动生成排版框",
         "status": "todo", "progress": 0, "notes": "装 Florence-2 + 调用节点，输出 0-1000 归一化框灌进排版器", "related_files": ["web/app.js", "src-tauri/src/main.rs"], "last_thread_id": "", "deps": [], "created_at": "2026-10-31 13:30"},
        {"id": 2, "priority": 3, "difficulty": 1, "title": "排版自定义模板保存到本地",
         "status": "todo", "progress": 0, "notes": "localStorage 或 settings 目录", "related_files": ["web/app.js"], "last_thread_id": "", "deps": [], "created_at": "2026-10-31 13:30"},
        {"id": 3, "priority": 3, "difficulty": 3, "title": "小助手接 generate/layout 工具（限 4 步）",
         "status": "todo", "progress": 0, "notes": "生成类工具放最后一步，失败要能回退到提示词模式", "related_files": ["src-tauri/src/enhance.rs"], "last_thread_id": "", "deps": [], "created_at": "2026-10-31 13:30"},
        {"id": 4, "priority": 3, "difficulty": 5, "title": "连载：Series/Shot 持久化 + 设定卡 + 整话导出",
         "status": "todo", "progress": 0, "notes": "剧情拆分之后的自然延伸", "related_files": [], "last_thread_id": "", "deps": [], "created_at": "2026-10-31 13:30"},
    ],
}
os.makedirs(os.path.join(root, ".codex"), exist_ok=True)
open(os.path.join(root, ".codex", "state.json"), "w", encoding="utf-8", newline="\n").write(
    json.dumps(state, ensure_ascii=False, indent=2)
)
print("docs written")
