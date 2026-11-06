// 提示词辅助器的额外两格：提示词体检 + 机写排版草稿。
// 卡片的「点一下做什么」由这里提供，prompthelper.js 只负责画格子。

window.PH_EXTRA = [
  {
    icon: "🩺",
    title: "提示词体检",
    desc: "扫一遍：通配符写错、LoRA 不在列表、质量词堆砌、超长",
    onclick: () => openLint(),
  },
  {
    icon: "🧯",
    title: "崩图诊断",
    desc: "看一张坏图 + 刚才的参数，说原因、给能直接应用的改法",
    onclick: () => openDiagnose(),
  },
  {
    icon: "📐",
    title: "排版草稿",
    desc: "用本机模型按当前画布写一份 MOSS 排版 JSON，再进画布微调",
    onclick: () => mossDraft(),
  },
  {
    icon: "🗂",
    title: "提示词库",
    desc: "存过的提示词都在这里：搜索 / 收藏 / 常用 / 去 AI 套话",
    onclick: () => $("promptsBtn").click(),
  },
  {
    icon: "📖",
    title: "使用说明",
    desc: "人话版说明书：一分钟出图、各功能怎么用、常见问题",
    onclick: () => openHelp(),
  },
  {
    icon: "🧾",
    title: "配方库",
    desc: "把模型 + LoRA + 参数 + 风格 + 排版存成一份，一键套回来",
    onclick: () => openRecipes(),
  },
];

async function mossDraft() {
  const request = ($("prompt").value || $("layoutSummary").value || "").trim();
  if (!request) return toast("先写一句想画什么", true);
  const w = Number($("width").value) || 1024;
  const h = Number($("height").value) || 1024;
  toast("正在写排版草稿…");
  try {
    const draft = await api.call("layout_draft", { request, width: w, height: h });
    const n = await layoutLoadJson(draft);
    if (n) toast(`排版草稿已载入（${n} 个框），拖一下位置再生成`);
  } catch (e) {
    toast(String(e), true);
  }
}
