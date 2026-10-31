p = r"D:\deepseek-files\ai-tools\comfy-shell\README.md"
s = open(p, encoding="utf-8").read()

import re
s = re.sub(r"## 现状（v0\.\d+\.\d+，2026-10-31 实测通过）", "## 现状（v0.29.0，2026-10-31 实测通过）", s)

row = ("| **双 LoRA** | ✅ 第二个 LoRA 槽，同架构串联，各自强度 |\n"
       "| **人像预设** | ✅ 真实感 / 自然耐看 / 生图感 / 精致人像，一键配好模型 + LoRA + 强度 |\n"
       "| **姿势库** | ✅ 内置 Shisei 白模姿势（280 组 × 5 机位，CC0）：点卡片加参考图、填/复制英文描述 |")
if "**人像预设**" not in s:
    s = s.replace("| **双 LoRA** | ✅ 第二个 LoRA 槽，同架构串联，各自强度 |", row, 1)

section = """## 姿势库（Shisei）

- 来源：[wquguru/shisei](https://github.com/wquguru/shisei)，CC0，280 组姿势 × 5 机位（正面/斜侧/侧面/俯视/仰视）
- 资源位置：`ComfyUI\\input\\shisei\\{line,photo,catalog.json}`
- 壳里点「参考图 → 🕺 姿势库」，可搜索、按类别筛选、切换机位
- **点卡片** = 该姿势白模加入参考图；卡片「**填**」= 英文姿势描述填进提示词；「**词**」= 复制描述
- 白模只负责“怎么站”，角色长相仍由你的提示词 / 参考图决定

## 训练"""
if "## 姿势库（Shisei）" not in s:
    s = s.replace("## 训练", section, 1)

open(p, "w", encoding="utf-8", newline="\n").write(s)
print("readme ok")
