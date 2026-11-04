// 每格补身份（#98）：先出图，再换脸 / 换头。
//
// 两条路都在壳里现成：
//   换脸 = FLUX 裁脸贴回（faceswap.rs 的 face_swap），保留这一格自己的发型/头饰；
//   换头 = Qwen-2.1 + BFS 换头 LoRA（faceswap.js 那套 qwen_head 配方），把角色卡的发饰一起带过来。
// 结果都追加进该格（shot_add_image），原图不删。

const SHOT_SWAP_FACE_PROMPT = "同一个人的脸，自然肤质，与原图一致的光照和肤色";

async function shotSwapCards() {
  try {
    return await api.call("character_list", { project: (window.projectUI && projectUI.current()) || null });
  } catch {
    return [];
  }
}

async function shotSwapCard() {
  const sel = $("shotSwapCard");
  const id = sel ? sel.value : "";
  const cards = await shotSwapCards();
  const card = cards.find((c) => c.id === id) || cards.find((c) => c.identityPlate);
  if (!card || !card.identityPlate) throw new Error("没有带身份底板的角色卡");
  return card;
}

async function shotSwapMode() {
  const el = document.querySelector('input[name="shotSwapMode"]:checked');
  return el ? el.value : "face";
}

async function shotSwapFillCards() {
  const sel = $("shotSwapCard");
  if (!sel) return;
  const cards = (await shotSwapCards()).filter((c) => c.identityPlate);
  sel.innerHTML = cards.map((c) => `<option value="${c.id}">${c.name}</option>`).join("");
  if (!cards.length) sel.innerHTML = '<option value="">（没有带底板的角色卡）</option>';
}

/// 换脸：这一格自己的发型/头饰不动，只把脸换成角色。
async function shotSwapFaceOne(s, card) {
  const src = (s.images || []).slice(-1)[0];
  if (!src) throw new Error("这一格还没有图");
  const dataUrl = await api.call("image_data_url", { path: src });
  const name = await api.call("save_ref_image", { name: "shot_swap_src.png", dataUrl });
  const box = await api.call("face_detect", { image: name });
  if (!box || !box.bbox) throw new Error("这一格里没找到脸（侧脸/背影换不了）");
  const refFace = await inpaintPlateFaceDataUrl(card.identityPlate);
  const res = await api.call("face_swap", {
    req: {
      source: src,
      face: box.bbox,
      prompt: SHOT_SWAP_FACE_PROMPT,
      negative: "",
      profile: "flux3",
      steps: 16,
      cfg: 1,
      seed: -1,
      denoise: 0.75,
      feather: 0,
      pad: 1.6,
      refImageDataUrl: refFace,
    },
  });
  const p = shotProject();
  await api.call("shot_add_image", { project: p.id, id: s.id, path: res.images[0].path });
  return res.images[0].path;
}

/// 换头：整颗头搬过来（发型 + 头饰 + 五官），Qwen-2.1 + BFS。
async function shotSwapHeadOne(s, card) {
  const src = (s.images || []).slice(-1)[0];
  if (!src) throw new Error("这一格还没有图");
  const sceneUrl = await api.call("image_data_url", { path: src });
  const sceneName = await api.call("save_ref_image", { name: "shot_head_scene.png", dataUrl: sceneUrl });
  const plateUrl = await api.call("image_data_url", { path: card.identityPlate });
  const plateName = await api.call("save_ref_image", { name: "shot_head_plate.png", dataUrl: plateUrl });
  const box = await api.call("face_detect", { image: plateName });
  if (!box || !box.bbox) throw new Error("角色底板上没找到脸");
  const plateImg = await qsImage(plateUrl);
  const headName = await api.call("save_ref_image", { name: "shot_head_crop.png", dataUrl: qsHeadCrop(plateImg, box.bbox) });
  const res = await api.call("generate", {
    req: {
      profile: "qwen21",
      mode: "edit",
      prompt: typeof QWEN_HEAD_PROMPT === "string" ? QWEN_HEAD_PROMPT : "head_swap: …",
      negative: "",
      width: 1024,
      height: 1024,
      steps: 8,
      cfg: 1,
      seed: -1,
      batch: 1,
      refImages: [sceneName, headName],
      lora: "bfs_head_v1.1_qwen_2.1.safetensors",
      loraStrength: 1,
      lora2: "p_qwen_image_2.1_8step_v0.1.safetensors",
      lora2Strength: 1,
      slg: false,
      slgLayers: "22",
      layout: null,
      sampler: "deis",
      editResolution: 0,
      project: (window.projectUI && window.projectUI.current()) || null,
      staging: false,
    },
    turbo: false,
  });
  const p = shotProject();
  await api.call("shot_add_image", { project: p.id, id: s.id, path: res.images[0].path });
  return res.images[0].path;
}

async function shotSwapOne(s) {
  const card = await shotSwapCard();
  const mode = await shotSwapMode();
  if (mode === "head") return shotSwapHeadOne(s, card);
  if (mode === "keep") return shotSwapHeadKeepOne(s, card);
  return shotSwapFaceOne(s, card);
}

async function shotSwapRun(shots, label) {
  const btn = $("shotsSwapAll");
  const old = btn ? btn.textContent : "";
  if (btn) btn.disabled = true;
  let ok = 0;
  let fail = 0;
  for (const s of shots) {
    if (btn) btn.textContent = `${label} ${ok + fail + 1}/${shots.length}（点一下停）`;
    try {
      await shotSwapOne(s);
      ok++;
    } catch (e) {
      fail++;
      console.warn("换失败", s.index, e);
    }
  }
  if (btn) {
    btn.disabled = false;
    btn.textContent = old || "整话执行";
  }
  await shotsRefresh();
  toast(`${label}：成功 ${ok} · 失败 ${fail}`, fail > 0);
}

$("shotsSwapAll").onclick = async () => {
  const card = await shotSwapCard().catch(() => null);
  if (!card) return toast("先建一个带身份底板的角色卡", true);
  const targets = shotsCache.filter((s) => (s.images || []).length);
  if (!targets.length) return toast("还没有出过图的格", true);
  const mode = await shotSwapMode();
  const label = mode === "head" ? "换头" : mode === "keep" ? "保构图换头" : "换脸";
  await shotSwapRun(targets, label);
};

$("shotsSwapCards").onclick = () => shotSwapFillCards();
shotSwapFillCards();

/// 换头但保构图（#97）：只把「头那一块」用局部重绘换成角色卡的发型与头饰，身体、衣服、背景原样。
/// 和整图换头的区别：不动构图，代价是只换得到头发/头饰，脸换得没那么「整」。
async function shotSwapHeadKeepOne(s, card, maskOverride) {
  const src = (s.images || []).slice(-1)[0];
  if (!src) throw new Error("这一格还没有图");
  let maskDataUrl = maskOverride || "";
  if (!maskDataUrl) {
    const dataUrl = await api.call("image_data_url", { path: src });
    const probe = await api.call("save_ref_image", { name: "shot_keep_probe.png", dataUrl });
    const box = await api.call("face_detect", { image: probe });
    if (!box || !box.bbox) throw new Error("这一格里没找到脸（侧脸/背影换不了，可以点「蒙」手动画）");
    const img = await qsImage(dataUrl);
    const W = img.naturalWidth;
    const H = img.naturalHeight;
    const fb = box.bbox;
    const bw = fb[2];
    const bh = fb[3];
    const cx = (fb[0] + bw / 2) * W;
    const faceH = Math.max(8, bh * H);
    // 头比脸框大得多（后脑勺的头发常在脸框外），蒙版给到 2.9 倍脸宽，否则会出现两个头
    const w = Math.max(32, bw * W * 2.9);
    const top = Math.max(0, fb[1] * H - faceH * 1.35);
    const bottom = Math.min(H, (fb[1] + bh) * H + faceH * 0.45);
    const x0 = Math.max(0, Math.min(W - w, cx - w / 2));
    const h = Math.max(32, bottom - top);
    const c = document.createElement("canvas");
    c.width = W;
    c.height = H;
    const g = c.getContext("2d");
    g.fillStyle = "#000000";
    g.fillRect(0, 0, W, H);
    g.fillStyle = "#ffffff";
    const r = Math.min(w, h) * 0.18;
    g.beginPath();
    if (g.roundRect) g.roundRect(x0, top, w, h, r);
    else g.rect(x0, top, w, h);
    g.fill();
    maskDataUrl = c.toDataURL("image/png");
  }

  const plateUrl = await api.call("image_data_url", { path: card.identityPlate });
  const plateImg = await qsImage(plateUrl);
  const plateProbe = await api.call("save_ref_image", { name: "shot_keep_plate.png", dataUrl: plateUrl });
  const plateBox = await api.call("face_detect", { image: plateProbe });
  if (!plateBox || !plateBox.bbox) throw new Error("角色底板上没找到脸");
  const refImageDataUrl = qsHeadCrop(plateImg, plateBox.bbox);

  const res = await api.call("inpaint_image", {
    req: {
      profile: "flux3",
      prompt:
        "只把这一块里的头发和头饰换成参考角色的发型与发饰（发簪、花钿）：保持原来头部的大小、位置与朝向，脸不要放大，不要改变构图，保留原来的光照和背景",
      negative: "不同的脸型，换脸，变形，多余的头",
      source: src,
      maskDataUrl,
      steps: 16,
      cfg: 1,
      seed: -1,
      denoise: 0.8,
      feather: 28,
      refImageDataUrl,
    },
  });
  const p = shotProject();
  await api.call("shot_add_image", { project: p.id, id: s.id, path: res.images[0].path });
  return res.images[0].path;
}


// 换方式时重画镜头列表：只有「换头·保构图」才显示手画蒙版按钮
document.querySelectorAll('input[name="shotSwapMode"]').forEach((el) => {
  el.addEventListener("change", () => {
    if (typeof shotsRefresh === "function") shotsRefresh();
  });
});
