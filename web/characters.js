// 角色卡：存「身份 + 造型 + 风格」，一键出 3:4 身份底板，再把底板当参考图。
//
// 为什么核心是「身份底板」而不是四视图板：
// 实测（设计笔记、设计笔记）——
// FLUX.3 编辑从一张 3:4 全身底板换装/换景，脸几乎不变；而从设定板切出来的窄条补白也救不了比例。

let charList = [];
let charCurrent = null;

function charBlank() {
  return {
    id: "",
    name: "",
    project: null,
    meta: { age: "", height: "", build: "", tags: [] },
    identityZh: "",
    identityEn: "",
    palette: [],
    outfits: [],
    identityPlate: null,
    consistency: "text",
    lora: null,
    graded: null,
  };
}

function charProjectId() {
  return window.projectUI && window.projectUI.current ? window.projectUI.current() : "";
}

async function charRefresh(keepId) {
  try {
    charList = await api.call("character_list", { project: charProjectId() || null });
  } catch (e) {
    charList = [];
    toast(`角色卡读不到：${e}`, true);
  }
  if (keepId) {
    const hit = charList.find((c) => c.id === keepId);
    if (hit) charCurrent = hit;
  }
  charRender();
}

function charRenderList() {
  const box = $("charListBox");
  box.textContent = "";
  if (!charList.length) {
    const d = document.createElement("div");
    d.className = "muted";
    d.style.cssText = "padding:10px 4px;font-size:12px";
    d.textContent = "还没有角色。填右边点「保存为角色」。";
    box.appendChild(d);
    return;
  }
  for (const c of charList) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "char-row" + (charCurrent && charCurrent.id === c.id ? " on" : "");
    const nm = document.createElement("b");
    nm.textContent = c.name;
    const sub = document.createElement("span");
    const bits = [];
    if (c.project) bits.push("本项目");
    else bits.push("全局");
    if (c.identityPlate) bits.push("有底板");
    if (c.consistency === "lora") bits.push("LoRA");
    sub.textContent = bits.join(" · ");
    row.append(nm, sub);
    row.onclick = () => {
      charCurrent = c;
      charRender();
    };
    box.appendChild(row);
  }
}

function charRender() {
  charRenderList();
  const c = charCurrent || charBlank();
  $("charName").value = c.name || "";
  $("charIdentityZh").value = c.identityZh || "";
  $("charIdentityEn").value = c.identityEn || "";
  $("charPalette").value = (c.palette || []).join("、");
  const o = (c.outfits || [])[0] || { version: "v1", desc: "" };
  $("charOutfit").value = o.desc || "";
  $("charConsistency").value = c.consistency || "text";
  $("charPlateState").textContent = c.identityPlate
    ? `底板：${c.identityPlate.split(/[\\/]/).pop()}`
    : "底板：还没有（点「出底板」用身份描述生成一张 3:4 全身图）";
  $("charDeleteBtn").hidden = !c.id;
}

function charCollect() {
  const c = charCurrent ? JSON.parse(JSON.stringify(charCurrent)) : charBlank();
  c.name = $("charName").value.trim();
  c.identityZh = $("charIdentityZh").value.trim();
  c.identityEn = $("charIdentityEn").value.trim();
  c.palette = $("charPalette").value.split(/[、,，\s]+/).filter(Boolean);
  const desc = $("charOutfit").value.trim();
  c.outfits = desc ? [{ version: (c.outfits && c.outfits[0] && c.outfits[0].version) || "v1", desc }] : [];
  c.consistency = $("charConsistency").value;
  if (!c.id) c.project = charProjectId() || null;
  return c;
}

async function charSave() {
  const c = charCollect();
  if (!c.name) {
    toast("先填角色名", true);
    return;
  }
  try {
    const saved = await api.call("character_save", { character: c });
    charCurrent = saved;
    await charRefresh(saved.id);
    toast(`已保存「${saved.name}」`);
  } catch (e) {
    toast(String(e), true);
  }
}

async function charDelete() {
  if (!charCurrent || !charCurrent.id) return;
  try {
    await api.call("character_delete", { id: charCurrent.id });
    charCurrent = null;
    await charRefresh();
    toast("已删除");
  } catch (e) {
    toast(String(e), true);
  }
}

/// 出身份底板：用身份描述 + 固定版式跑一张 3:4 全身图，出完自动设成底板并挂成参考图。
async function charMakePlate() {
  const c = charCollect();
  if (!c.identityZh) {
    toast("先填身份描述（中文），底板要用它", true);
    return;
  }
  const wasModel = $("model").value;
  const wasMode = state.mode;
  const platePrompt = [
    "纯白色无缝背景，全身站姿，人物完整不裁切头脚，标准头身比",
    c.identityZh,
    c.outfits && c.outfits[0] ? `服装：${c.outfits[0].desc}` : "",
  ]
    .filter(Boolean)
    .join("，");
  const btn = $("charPlateBtn");
  btn.disabled = true;
  btn.textContent = "出图中…";
  try {
    if (profile() && profile().id !== "flux3") {
      $("model").value = "flux3";
      $("model").dispatchEvent(new Event("change"));
      await new Promise((r) => setTimeout(r, 400));
    }
    setMode("t2i");
    const oldPrompt = $("prompt").value;
    $("width").value = 1024;
    $("height").value = 1365;
    $("prompt").value = platePrompt;
    const ok = await generate();
    $("prompt").value = oldPrompt;
    if (!ok) throw new Error("生成没成功");
    const first = (state.results || [])[0];
    if (!first) throw new Error("没拿到结果");
    c.identityPlate = first.path || null;
    c.consistency = "reference";
    const saved = await api.call("character_save", { character: c });
    charCurrent = saved;
    await charRefresh(saved.id);
    if (first.dataUrl) {
      await addRefFromDataUrl(first.dataUrl, `角色_${saved.name}`);
    }
    toast("底板已生成，并挂成参考图");
  } catch (e) {
    toast(`出底板失败：${e}`, true);
  } finally {
    btn.disabled = false;
    btn.textContent = "出底板";
    if (wasModel && $("model").value !== wasModel) {
      $("model").value = wasModel;
      $("model").dispatchEvent(new Event("change"));
    }
    if (wasMode === "edit") setMode("edit");
  }
}

/// 把已有的底板挂成参考图（重启后也能用）。
async function charUsePlate() {
  const c = charCollect();
  if (!c.identityPlate) {
    toast("这个角色还没有底板", true);
    return;
  }
  try {
    const dataUrl = await api.call("image_data_url", { path: c.identityPlate });
    await addRefFromDataUrl(dataUrl, `角色_${c.name}`);
    toast("底板已挂成参考图");
  } catch (e) {
    toast(String(e), true);
  }
}

function charOpen() {
  $("charModal").hidden = false;
  charRefresh(charCurrent && charCurrent.id);
}

// ---------------------------------------------------------------- @角色 引用

/// 确保角色列表已加载（第一次用到时才拉，不在启动时打扰）。
async function charEnsureLoaded() {
  if (charList.length) return charList;
  try {
    charList = await api.call("character_list", { project: charProjectId() || null });
  } catch {
    charList = [];
  }
  return charList;
}

/// 把提示词里的 @名字 换成身份描述，返回 {text, matched:[角色...], missing:[名字...]}。
async function charResolveTokens(raw) {
  const cats = await charEnsureLoaded();
  const matched = [];
  const missing = [];
  const text = raw.replace(/@([\u4e00-\u9fa5A-Za-z0-9_\-]{1,20})/g, (whole, name) => {
    const hit = cats.find((c) => c.name === name) || cats.find((c) => c.name.startsWith(name));
    if (!hit) {
      missing.push(name);
      return whole;
    }
    if (!matched.some((c) => c.id === hit.id)) matched.push(hit);
    const bits = [hit.identityZh];
    const outfit = hit.outfits && hit.outfits[0] && hit.outfits[0].desc;
    if (outfit) bits.push(`服装：${outfit}`);
    return bits.filter(Boolean).join("，");
  });
  return { text, matched, missing };
}

/// 把角色的身份底板挂成参考图（最多 3 张）。
async function charAttachPlates(chars) {
  const plates = chars.map((c) => c.identityPlate).filter(Boolean).slice(0, 3);
  if (!plates.length) return [];
  const names = [];
  for (const p of plates) {
    const dataUrl = await api.call("image_data_url", { path: p });
    names.push(await api.call("save_ref_image", { name: "char_plate.png", dataUrl }));
  }
  return names;
}

// 包一层 generate：有 @角色 就展开身份、挂底板、切到 FLUX.3 编辑
const charBaseGenerate = generate;
generate = async function charGenerate() {
  const ta = $("prompt");
  const raw = ta.value;
  if (!raw.includes("@")) return charBaseGenerate();

  let resolved;
  try {
    resolved = await charResolveTokens(raw);
  } catch {
    return charBaseGenerate();
  }
  if (resolved.missing.length) {
    toast(`没有叫「${resolved.missing.join("、")}」的角色卡`, true);
  }
  if (!resolved.matched.length) return charBaseGenerate();

  const wasMode = state.mode;
  const wasRefs = state.refs.slice();
  ta.value = resolved.text;
  try {
    // 只有「不能吃参考图」的模型才强制切走；能编辑的（qwen21 / flux3base）尊重用户选择
    const p0 = profile();
    if (p0 && !p0.supportsEdit) {
      const w0 = $("width").value;
      const h0 = $("height").value;
      $("model").value = "flux3";
      $("model").dispatchEvent(new Event("change"));
      await new Promise((r) => setTimeout(r, 300));
      $("width").value = w0;
      $("height").value = h0;
      toast(`${p0.label || p0.id} 不能吃参考图，已切到 FLUX.3 编辑`);
    } else if (p0 && p0.id === "qwen21") {
      toast("Qwen 编辑也能保造型，但比 FLUX.3 慢很多（约 2-4 分钟/张）");
    }
    const plates = await charAttachPlates(resolved.matched);
    if (plates.length) {
      state.refs = plates;
      renderRefs();
      if (profile() && profile().supportsEdit && state.mode !== "edit") setMode("edit");
    } else {
      toast(`「${resolved.matched[0].name}」还没有底板，这次只注入了文字（点「出底板」会更稳）`);
    }
    return await charBaseGenerate();
  } finally {
    ta.value = raw;
    state.refs = wasRefs;
    renderRefs();
    if (wasMode !== state.mode) setMode(wasMode);
  }
};

function charInit() {
  $("charBtn").onclick = () => charOpen();
  $("charClose").onclick = () => ($("charModal").hidden = true);
  $("charModal").addEventListener("click", (e) => {
    if (e.target === $("charModal")) $("charModal").hidden = true;
  });
  $("charNewBtn").onclick = () => {
    charCurrent = null;
    charRender();
    $("charName").focus();
  };
  $("charSaveBtn").onclick = () => charSave();
  $("charDeleteBtn").onclick = () => charDelete();
  $("charPlateBtn").onclick = () => charMakePlate();
  $("charUsePlateBtn").onclick = () => charUsePlate();
  $("charRefBtn").onclick = () => {
    const name = $("charName").value.trim();
    if (!name) {
      toast("先填角色名", true);
      return;
    }
    const box = $("prompt");
    const cur = box.value.trim();
    box.value = cur ? `${cur}，@${name}` : `@${name}`;
    box.focus();
    box.setSelectionRange(box.value.length, box.value.length);
    $("charModal").hidden = true;
    toast(`已插入 @${name}`);
  };
}

charInit();
