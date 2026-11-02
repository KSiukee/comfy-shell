# 显影（ComfyShell）

只干活的本地生图/改图壳：**不打开 ComfyUI 界面**，后台静默拉起 ComfyUI，界面上只有提示词、参数和结果。
Rust（Tauri 2）后端 + 纯静态前端（无 node 构建）。

> 第一次用？先看 [使用说明（人话版）](web/使用说明.md)——一分钟出第一张图、提示词辅助怎么用、镜头和排版怎么上手。

## 现状（v0.94.0）

| 能力 | 状态 |
|---|---|
| 静默拉起 / 健康检查 / 关闭 ComfyUI | ✅ 无黑窗；**启动不阻塞 UI**，点生成会排队等它就绪 |
| 模型 | Qwen-Image 2.1 / FLUX.3 Klein（蒸馏、Base）/ MOSS-Image SFT、Base / 自定义 API 工作流 |
| 生图 / 改图 | ✅ 参考图最多 10 张（主体/风格），Qwen 2.1 与 FLUX.3 支持 |
| 提示词扩写 | ✅ 官方 PE；被拒时自动换本地 heretic 重写，设置里可强制走 heretic |
| 负向提示词 | ✅ 可中→英一键翻译 |
| 自动识框 | ✅ 海报/菜单/截图 → 文字块 + 物体框（Qwen3-VL，带 OCR 原文，底图对位） |
| 排版模板 | ✅ 存本机 `layouts\*.json` / 下拉载入 / 删除；0..1 归一化框，换尺寸不用重画 |
| 去 AI 感 | ✅ 一键补中性白平衡 / 无文字 / 自然表情 / 单光源 / 浅景深 + 噪点、轻微畸变；负面词同步补，按片段去重 |
| 步数 / CFG | ✅ 同行对齐，30/36/42/49/25/50 快捷点选 |
| 双 LoRA | ✅ 第二个 LoRA 槽，同架构串联，各自强度 |
| 采样 | MOSS 50 步 / CFG 3 / euler / **SLG skip 22**；Qwen 45 步 / CFG 4；FLUX 蒸馏 16、Base 50 |
| 提示词库 | ✅ 跨项目保存/搜索/收藏/常用；去AI套话；私密项目不写入 |
| 版本 | ✅ 项目内谱系（生成自动记账、默认 1 星、1–5 星、按时间/星排序、父版本 diff、从旧版分叉）；排版版本 kind=layout / MOSS；私密项目不记 |
| 镜头 | ✅ 剧情拆分存镜头；填入/单格重画（换种子）/删除；整话导出（最新图+story.md）；私密项目暂不支持 |
| 队列 | ✅ 条目列表/暂停/重排/单条移除/取消当前/ETA；Problems 面板集中错误 |
| 工作区 | ✅ 每项目保存表单/模式/排版/面板/图库筛选，切项目/重启恢复；私密不保存 |
| 元数据 | ✅ 生成/改图/超分图写 PNG iTXt；拖回窗口恢复表单/项目；私密项目不写 |
| 数据 | ✅ 备份（只含密文 vault）/ 导出项目（私密需解锁+确认，标 plaintext）/ 导入项目（新 id） |
| 锁定区 | ✅ 口令加密（Argon2id + XChaCha20）；配置+图片移入/移出；私密项目可生成（staging→加密→删明文）；图库私密档；10 分钟自动锁 |
| 图库 | ✅ 缓存（磁盘+内存）+ 分批渲染；缩略图大小滑条；收藏会落盘到 `output\_favorites` |
| 图库搜索 | ✅ 文件名 / 生成提示词；中文查询可「中→英」二次搜索 |
| 大图查看 / 管理 | ✅ 缩放、拖拽、←/→/滚轮翻页、一键 4× 超分；右键删除/定位/当参考图/复制提示词 |
| 调色 / 裁切 / LUT | ✅ 图库、图区、大图共用一页：拖框裁切 + 比例预设 + 旋转；套 `.cube` LUT（三线性 + 强度）；曝光/对比/高光/阴影/饱和/色温/色调/褪色/暗角。原图不动，按原分辨率另存到原图同目录 |
| 超分 | ✅ 4x ESRGAN 四个模型 + SeedVR2 3B（AI 修复）；支持图库批量与外部文件夹 |
| 局部重绘 | ✅ 画笔蒙版、羽化、洞里塞参考图（Qwen 2.1） |
| 训练（LoRA） | ✅ 内置（FLUX）与 kohya（MOSS/大分辨率）双引擎；逐图标注/绿红圈选/自动 caption/续训/固定评测/实验记录 |
| 剧情拆分 | ✅ 本地 Qwen3.5 把剧情拆成连续分镜，人工确认后推批量队列 |
| 批量队列 / 模型横扫 | ✅ 多行排队；同提示词多模型各出一张 |
| A/B 对比 / 导出对比图 | ✅ 图库选两张对比，可导出拼图 |

## 启动与性能

- auto-start 在后台线程拉起 ComfyUI；状态查询是异步命令，壳启动即可点
- 生成/队列前会 `ensureReady()` 轮询到 ComfyUI 就绪，再发任务（最多等 5 分钟）
- 图库按「输出目录 mtime + stars mtime」做内存 + `library_cache.json` 缓存；界面一次渲染 120 张，滚动追加
- 实测 251 张：重复扫描 **0.2 秒**；启动 3 秒内 UI 可交互

## 采样参数（实测口径）

| 模型 | 步数 | CFG | 采样器 | 备注 |
|---|---|---|---|---|
| MOSS SFT / Base | **50** | **3.0** | **euler** | 官方：≥50 步、CFG 3–6、shift 5、SLG skip 22；shift 5 在模型内部，无需外挂 |
| Qwen 2.1 | **45** | **4.0** | euler | 低于 40 结构容易塌；CFG=1 时负面词无效 |
| FLUX.3 蒸馏 | 16 | 1.0 | euler | turbo LoRA 8 步 |
| FLUX.3 Base | 50 | 3.5 | euler | 未蒸馏，慢但稳 |

MOSS 的 **SLG** 用 ComfyUI 通用 `SkipLayerGuidanceDiT`（`single_layers=22, scale=3`），默认开，慢约 25%，暗部/细节更实。

## 去 AI 感

- 提示词旁「🎞 去AI感」：一键补中性白平衡、无文字/水印、不看镜头的自然表情、单光源软阴影、f1.8 浅景深、轻微胶片颗粒与镜头畸变
- 负面词同时补：黄光 / 过饱和 / 发光 / 塑料皮肤 / 过度磨皮等
- 按逗号片段去重：提示词里已有的词不重复追加，点两次等于一次
- 只改提示词，不动步数/CFG/模型；适合人像「一眼 AI」时打底，再按需删改

## 姿势库（Shisei）

- 来源：[wquguru/shisei](https://github.com/wquguru/shisei)，CC0，280 组姿势 × 5 机位（正面/斜侧/侧面/俯视/仰视）
- 资源位置：`ComfyUI\input\shisei\{line,photo,catalog.json}`
- 多 pack：`ComfyUI/input/pose_packs/<id>/{pack.json,catalog.json,line|images}`；旧 `input/shisei` 自动作为 legacy pack，UI 里可切包
- 第三方库适配：`python tools/pose_pack_adapter.py --selftest` / 用法见 `docs/姿势库-pack适配.md`
- 已接 ai-pose 中文包：288 组（本地 junction；图片/数据非 MIT，仅本机非商用）
- 授权标记：pack.json 的 `publishable/license/note`；ai-pose 标「仅本地」，导出会二次确认
- 壳里点「参考图 → 🕺 姿势库」，可搜索、按类别筛选、切换机位
- **点卡片** = 该姿势白模加入参考图；卡片「**填**」= 英文姿势描述填进提示词；「**词**」= 复制描述
- 白模只负责“怎么站”，角色长相仍由你的提示词 / 参考图决定

## 本地小助手（实验）

- 提示词旁「🤖 助手」：用中文说需求，本地 Qwen3.5-9B 走小型工具循环
- 四个工具：`translate`（中→英片段）、`expand`（本地 heretic 扩写）、`pose`（搜姿势库）、`analyze`（拆当前参考图），最多 4 步
- 实测「雨夜街头穿红大衣的银发女孩，电影感」→ translate → expand → 约 20 秒出英文提示词
- 只是固定工具的迷你 agent，不是通用 skill 加载器；`generate` / `layout` 还没接（见待办）

## MOSS 排版（Layout JSON）

- 提示词旁「🎨 排版」（仅 MOSS 系模型）：拖框标出每个元素，文字和物体分开
- 可加多个元素，每个有描述；文字元素还能填要渲染的文字
- 点「按排版生成」把元素换算成 MOSS 官方 layout JSON（abs_centralized / yxyx），直接作为提示词
- 实测 512² / 12 步：标题「MOSS」和红杯子位置都按框落位
- **📷 识框**：选一张海报 / 菜单 / 截图，本地 Qwen3-VL 自动给出文字块 + 物体框，文字块带原文；底图铺在画布下对位
- 实测 768² 五元素海报 5/5 找到；框是草稿级（偏紧、个别偏 ~0.1），拖角微调后再生成
- **我的模板**：底部填模板名 →「存为模板」，落到 `%APPDATA%\com.dsh.comfyshell\layouts\<名>.json`（一模板一文件，可直接拷给别人）；下拉「我的模板」载入，「删除模板」只删模板文件

## 看图拆提示词

- 提示词旁的「🔍 解析图」：选一张图 → 本地 Qwen3-VL 输出九类结构化描述
- 九类：姿势 / 表情 / 衣着 / 配饰 / 背景 / 风格 / 色彩 / 光线 / 氛围，每类可手改
- 模型可自行追加「额外分类」（如 weather=rain、time=night），也可手动「＋ 加分类」
- 底部折叠区放「完整提示词」和「原始 JSON」；可一键组合进提示词或复制 JSON
- 实测亚洲夜景人像：姿势、衣着、色彩、光线、氛围都准；「姿势」偶尔会把场景混进来，「配饰」常为空
- 首次加载视觉模型约 30–60 秒，之后几秒

## 横扫对比

- 两种模式：**按人像预设**（真实感 / 自然耐看 / 生图感 / 精致人像）或**按模型**
- 尺寸：当前尺寸，或 **5 档（512/640/768/896/1024）** 自动跑一遍
- 张数：1/2/3 张一组（batch）
- 实测：4 个预设各 1 张约 90 秒；5 档 × 2 张共 10 张（512→1024）全部出图
- 注意：**同 seed、batch 2 的两张内容并不相同**（实测像素均差 37），做严格对照请用 batch 1；1024×2 明显慢

## 训练

- **引擎**：训练弹窗「训练引擎」= 自动 / 内置 / kohya。自动模式下 MOSS 走 kohya，其它走内置
- **内置**：ComfyUI `TrainLoraNode`；FLUX.3 Base bf16，512 约 5 秒/步，640 约 12 秒/步，768 OOM
- **kohya**：`ai-tools\sdvenv` + `sd-scripts\moss_image_train_network.py`，int8 权重 + `--blocks_to_swap 20` + 缓存 TE，512 约 12 秒/步
- **逐图标注**：每张图可写「要 / 不要 / 权重」，点缩略图可圈**绿区=只保留**、**红区=训练前抹掉**
- **自动 caption**：本地 Qwen3-VL 逐张描述
- **续训**：选已有 LoRA 继续练（新文件名）
- **评测**：训练后固定 prompt × seed 出对比图；每次训练存 manifest 到 `%APPDATA%\com.dsh.comfyshell\experiments`
- 训练前会重启 ComfyUI 清显存（显存不足必勾）

## 已知限制

- MOSS 内置 trainer 只能 384；512 要用 kohya
- MOSS/Qwen/FLUX 的 LoRA **互不通用**（绑架构），数据集可复用
- 训练尺寸 768 在 16G 上 OOM；建筑类建议 640 + 等比镜像填充
- 两个 LoRA 是串联叠加，不是权重融合；同一触发词会互相抢
- 局部重绘的红区是模糊而非真掩码训练（ComfyUI 训练节点不支持 masked loss）
- 剧情拆分只到「拆格 + 推队列」，还没有 Series/Shot 持久化

## 开发脚本

```
python dev\test_lifecycle.py   # 启停/健康检查
python dev\test_features.py    # 生图/扩写/参考图
python dev\test_v04.py         # 搜索/导出对比/批量改图/横扫
python dev\test_v05.py         # 中→英搜索/查看器/超分
python dev\test_v06.py         # 画笔蒙版 + 局部重绘
python dev\test_v07_ui.py      # 训练 LoRA（内置）
python dev\test_v09_folder.py  # 外部文件夹训练 + 进度 + 损失曲线
python dev\test_v11_resume.py  # 续训
python dev\test_v12_final.py   # 640 等比镜像填充
python dev\test_v18_crop.py    # 框选裁切
python dev\test_v20_mark.py    # 绿/红标注 + 标注翻译
python dev\test_v23.py         # 双 LoRA / 收藏落盘 / SLG 前置检查
python dev\probe_slg.py        # MOSS SLG 开关对比
```

配置：`%APPDATA%\com.dsh.comfyshell\{settings.json, history.json, stars.json, library_cache.json, workflows\, experiments\, kohya_runs\}`

模型目录：`ComfyUI\models\{diffusion_models, text_encoders, vae, loras, upscale_models}`

## 后续（接筑光）

`api.rs` 的工作流构建 + 提交 + 轮询 + 取图可以整体搬进筑光的修图流水线；连载功能（Series/Shot 持久化、设定卡、整话导出）还没做。

