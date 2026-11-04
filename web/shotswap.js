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
  return mode === "head" ? shotSwapHeadOne(s, card) : shotSwapFaceOne(s, card);
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
  await shotSwapRun(targets, mode === "head" ? "换头" : "换脸");
};

$("shotsSwapCards").onclick = () => shotSwapFillCards();
shotSwapFillCards();
