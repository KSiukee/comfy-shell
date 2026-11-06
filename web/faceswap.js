// 换脸/ Qwen 换头 · 换身。
//
// 三种做法同一个入口，靠「做法」下拉切换：
//   flux      —— 裁脸 → 整块重绘 → 羽化贴回（后端 faceswap.rs）。只换脸，头发/衣服/背景逐像素保留。
//   qwen_head —— Qwen-2.1 + BFS 换头 LoRA：整颗头（脸/发型/头饰）过来，身体姿势光照背景由原图约束。
//   qwen_body —— Qwen-2.1 + BFS 换身 LoRA：整套造型（衣服/体型/脸）过来，姿势构图背景保留。
//
// Qwen 那两条都走普通「生成」通道（generate, profile=qwen21, mode=edit, 两张参考），
// 后端只需要：qwen21 编辑分支放行第二张参考 + 允许传 sampler / edit_resolution。
// 参数照 BFS 官方工作流：8 步 + cfg1 + deis + 加速 LoRA；场景/参考的比例按官方文档
// （换身：场景 ~2MP、参考 ~0.59MP；换头：参考紧裁到头再缩到 512）。

const faceSwap = { bbox: null, refDataUrl: "", plateCard: null, plateLabel: "" };

const QWEN_HEAD_LORA = "bfs_head_v1.1_qwen_2.1.safetensors";
const QWEN_BODY_LORA = "bfs_body_swap_v1.0_qwen_2.1.safetensors";
const QWEN_ACCEL_LORA = "p_qwen_image_2.1_8step_v0.1.safetensors";

const QWEN_HEAD_PROMPT =
  "head_swap: start with <image1> as the base image, keeping its lighting, environment, and background. " +
  "remove the head from <image1> completely and replace it with the head from <image2>, strictly preserving " +
  "the hair, eye color, nose structure from <image2>. copy the direction of the eye, head rotation, micro " +
  "expressions from <image1>, high quality, sharp details, 4k";
const QWEN_BODY_PROMPT =
  "body_swap: start with <image1> as the base image, keeping its lighting, environment, and background. " +
  "replace the body from <image1> with the body from <image2>, strictly preserving the clothing, body shape " +
  "and proportions from <image2>. copy the pose, direction of the eye, head rotation, micro expressions from <image1>";

const FACE_HINTS = {
  flux:
    "做法：裁出整张脸（外扩 1.6 倍）→ 让模型重画成一张完整的脸 → 羽化贴回。蒙版不再横穿眉眼，所以不会出现重影/半条眉毛。",
  qwen_head:
    "做法：整张图交给 Qwen-2.1 + BFS 换头 LoRA——脸、发型、头饰整套换成参考（底板会先紧裁到头），身体、姿势、光照、背景由原图约束。8 步约 10 秒。",
  qwen_body:
    "做法：整张图交给 Qwen-2.1 + BFS 换身 LoRA——衣服、体型、脸整套换成参考，姿势/构图/背景保留。参考要全身正面（角色底板本身就是）。2MP 输出约 45 秒。",
};
const FACE_HINTS_BOTTOM = {
  flux: "结果另存一张（原图不动）。0.70-0.80 保留原图的光照和皮肤质感；0.85 以上会把整张脸重画，底板上的花钿/发饰也容易被一起画上去。提示词里写「花钿 / 发饰 / 头饰」= 真的要它们。",
  qwen: "结果另存一张（原图不动）。8 步要挂加速 LoRA（p_qwen_image_2.1_8step）；没挂就跑 45 步/cfg 4。下面两个勾会往提示词里补「保持姿势 / 保持头转向」两句。",
};

function faceMode() {
  return $("faceMode") ? $("faceMode").value : "flux";
}
function faceIsQwen() {
  return faceMode().startsWith("qwen");
}
function faceQwenBasePrompt() {
  return faceMode() === "qwen_body" ? QWEN_BODY_PROMPT : QWEN_HEAD_PROMPT;
}
function faceQwenFinalPrompt() {
  let p = $("facePrompt").value.trim();
  if ($("faceKeepPose") && $("faceKeepPose").checked) {
    p += " keep the exact body pose of <image1>: same limb positions, same viewing angle, do not change the pose.";
  }
  if ($("faceKeepHead") && $("faceKeepHead").checked) {
    p +=
      " keep the head turned exactly as in <image1>: profile view, looking away from the camera, " +
      "NOT looking at the camera, same head rotation and neck angle.";
  }
  return p;
}

function faceSwapTarget() {
  const p = profile();
  if (p && p.supportsEdit && p.id !== "mossedit") return p;
  return (
    (state.profiles || []).find((x) => x.id === "flux3") || {
      id: "flux3",
      label: "FLUX.3 Klein（蒸馏）",
      steps: 16,
      cfg: 1.0,
      supportsEdit: true,
    }
  );
}

// 提示词只写「脸本身」：肤质、光照、肤色。
// 别把角色卡的 identityZh 拼进来，也别写「头饰/发型/服装」——模型只能画蒙版里那块
// 脸，这些词会被它当成「往脸上画」的指令（实测：角色卡写着「额间红色花钿…白色珍珠
// 花朵发饰」，换出来的脸就真的长出了花钿、发簪和红穗耳坠）。
function faceSwapDefaultPrompt() {
  return "同一个人的脸，自然肤质，与原图一致的光照和肤色";
}

function faceSyncMode() {
  const qwen = faceIsQwen();
  const bodyMode = faceMode() === "qwen_body";
  $("faceFluxRow").hidden = qwen;
  $("facePadField").hidden = qwen;
  $("faceCfgField").hidden = !qwen;
  $("faceQwenRow").hidden = !qwen;
  if ($("faceModeHint")) $("faceModeHint").textContent = FACE_HINTS[faceMode()] || FACE_HINTS.flux;
  if ($("faceHint")) $("faceHint").textContent = qwen ? FACE_HINTS_BOTTOM.qwen : FACE_HINTS_BOTTOM.flux;
  if (qwen) {
    $("faceSteps").value = 8;
    $("faceCfg").value = "1";
    $("facePrompt").value = faceQwenBasePrompt();
    if ($("faceKeepPose")) $("faceKeepPose").checked = bodyMode;
    if ($("faceKeepHead")) $("faceKeepHead").checked = false;
  } else {
    const t = faceSwapTarget();
    if (t) $("faceSteps").value = t.steps;
    $("facePrompt").value = faceSwapDefaultPrompt();
  }
}

async function faceSwapDetect() {
  const it = viewer.list[viewer.idx];
  if (!it) throw new Error("没有当前图片");
  const dataUrl = await api.call("image_data_url", { path: it.path });
  const name = await api.call("save_ref_image", { name: "face_detect_src.png", dataUrl });
  const box = await api.call("face_detect", { image: name });
  const bbox = box && box.bbox;
  if (!Array.isArray(bbox) || bbox.length !== 4) throw new Error("模型没给出脸框");
  return bbox;
}

async function openFaceSwap() {
  const it = viewer.list[viewer.idx];
  if (!it) return;
  if (String(it.path).startsWith("vault://")) {
    toast("私密项目里的图暂不支持换脸", true);
    return;
  }
  if (maskState.active) exitMaskMode();
  const target = faceSwapTarget();
  if (!target) {
    toast("没有能改图的模型：先在顶部切到 FLUX.3", true);
    return;
  }
  faceSwap.bbox = null;
  faceSwap.refDataUrl = "";
  faceSwap.plateCard = null;
  $("faceRefState").textContent = "参考：未选（只靠提示词，样子不稳）";
  $("faceDenoise").value = "0.75";
  $("faceFeather").value = "0";
  $("facePad").value = "1.6";
  faceSyncMode();
  // 不在这里预先找脸：找脸会禁用「开始」，而用户可能马上切到 Qwen 模式，按钮就一直是灰的
  // （实测踩过：换身那单点了开始没反应）。找脸推迟到「开始」里，flux 第一次开始时做。
  $("faceGo").disabled = false;
  $("faceModal").hidden = false;
  $("faceInfo").textContent = `${it.filename} · ${faceIsQwen() ? "选好参考后点开始" : "点开始（会先找脸）"}`;
}

async function faceSwapUsePlate() {
  const card = await pickCharacterPlate();
  if (!card) {
    toast("还没有带底板的角色卡：先去「角色卡」建一个并点「出底板」", true);
    return;
  }
  const btn = $("faceUsePlate");
  btn.disabled = true;
  const old = btn.textContent;
  btn.textContent = "处理中…";
  try {
    faceSwap.plateCard = card;
    if (faceIsQwen()) {
      faceSwap.plateLabel = `${card.name} 的底板（${faceMode() === "qwen_body" ? "全身" : "头部"}）`;
      $("faceRefState").textContent = `参考：${faceSwap.plateLabel}`;
      toast(`已用「${card.name}」的底板当参考`);
    } else {
      faceSwap.refDataUrl = await inpaintPlateFaceDataUrl(card.identityPlate);
      faceSwap.plateLabel = `${card.name} 的底板（已裁脸）`;
      $("faceRefState").textContent = `参考：${faceSwap.plateLabel}`;
      toast(`已用「${card.name}」的底板裁出脸当参考`);
    }
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = old;
  }
}

// ---------------------------------------------------------------- Qwen 两条

function qsImage(dataUrl) {
  return new Promise((res, rej) => {
    const img = new Image();
    img.onload = () => res(img);
    img.onerror = () => rej(new Error("图片解码失败"));
    img.src = dataUrl;
  });
}

function qsCanvasOut(img, sx, sy, sw, sh, w, h) {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const g = c.getContext("2d");
  g.imageSmoothingEnabled = true;
  g.imageSmoothingQuality = "high";
  g.drawImage(img, sx, sy, sw, sh, 0, 0, w, h);
  return c.toDataURL("image/png");
}

/// 按「总像素」缩放（BFS 官方文档：换身场景 ~2MP、参考 ~0.59MP），边长取 32 的倍数。
function qsScaleToMp(img, mp) {
  const W = img.naturalWidth;
  const H = img.naturalHeight;
  const k = Math.sqrt((mp * 1e6) / Math.max(1, W * H));
  const snap = (v) => Math.max(32, Math.round((v * k) / 32) * 32);
  return qsCanvasOut(img, 0, 0, W, H, snap(W), snap(H));
}

/// 从底板里紧裁一颗头（脸框外扩 1.6 倍，正方形，512²）——头肩裁切会把衣服染进结果里。
function qsHeadCrop(img, bbox) {
  const W = img.naturalWidth;
  const H = img.naturalHeight;
  const [bx, by, bw, bh] = bbox;
  const side = Math.max(64, Math.round(Math.max(bw * W, bh * H) * 1.6));
  const s = Math.min(side, W, H);
  const cx = (bx + bw / 2) * W;
  const cy = (by + bh / 2) * H;
  const x = Math.max(0, Math.min(W - s, Math.round(cx - s / 2)));
  const y = Math.max(0, Math.min(H - s, Math.round(cy - s / 2)));
  return qsCanvasOut(img, x, y, s, s, 512, 512);
}

function qsHasLora(name) {
  const norm = (s) => String(s).split(/[\\/]/).pop().toLowerCase();
  return ((state.models && state.models.loras) || []).some((l) => norm(l) === norm(name));
}

async function qwenPrepareRefs(it) {
  const bodyMode = faceMode() === "qwen_body";
  const card = faceSwap.plateCard || (await pickCharacterPlate());
  if (!card || !card.identityPlate) {
    throw new Error("还没有带底板的角色卡：先去「角色卡」建一个并点「出底板」");
  }
  faceSwap.plateCard = card;

  const sceneUrl = await api.call("image_data_url", { path: it.path });
  const sceneImg = await qsImage(sceneUrl);
  // 换头：场景保持原尺寸（脸要大）；换身：场景缩到 ~2MP
  const sceneOut = bodyMode ? qsScaleToMp(sceneImg, 2.0) : sceneUrl;

  const plateUrl = await api.call("image_data_url", { path: card.identityPlate });
  const plateImg = await qsImage(plateUrl);
  let refOut;
  if (bodyMode) {
    refOut = qsScaleToMp(plateImg, 0.59);
  } else {
    // 头：先让本机视觉模型给脸框，再紧裁
    const name = await api.call("save_ref_image", { name: "qs_plate_probe.png", dataUrl: plateUrl });
    const box = await api.call("face_detect", { image: name });
    refOut = qsHeadCrop(plateImg, box.bbox);
  }
  return { sceneOut, refOut, card, bodyMode };
}

async function qwenRun(it, btn) {
  const qwenProfile = (state.profiles || []).find((p) => p.id === "qwen21");
  if (!qwenProfile) throw new Error("没有 Qwen-Image 2.1 这一档：先在模型页确认 qwen_image_2.1 权重在");
  const bodyMode = faceMode() === "qwen_body";
  const lora = bodyMode ? QWEN_BODY_LORA : QWEN_HEAD_LORA;
  if (!qsHasLora(lora)) throw new Error(`models/loras 里没有 ${lora}（BFS 换头/换身权重，需自己放进 ComfyUI/models/loras）`);
  const hasAccel = qsHasLora(QWEN_ACCEL_LORA);

  btn.textContent = "准备参考…";
  const { sceneOut, refOut, card } = await qwenPrepareRefs(it);
  const stamp = Date.now();
  const sceneName = await api.call("save_ref_image", { name: `qs_scene_${stamp}.png`, dataUrl: sceneOut });
  const refName = await api.call("save_ref_image", { name: `qs_ref_${stamp}.png`, dataUrl: refOut });

  const steps = Number($("faceSteps").value) || 8;
  const cfg = Number($("faceCfg").value) || 1;
  const res = await api.call("generate", {
    req: {
      profile: "qwen21",
      mode: "edit",
      prompt: faceQwenFinalPrompt(),
      negative: "",
      width: it.width || 1024,
      height: it.height || 1024,
      steps,
      cfg,
      seed: seedArg(),
      batch: 1,
      refImages: [sceneName, refName],
      lora,
      loraStrength: 1.0,
      lora2: hasAccel ? QWEN_ACCEL_LORA : null,
      lora2Strength: 1.0,
      slg: false,
      slgLayers: "22",
      layout: null,
      sampler: hasAccel ? "deis" : null,
      editResolution: 0,
      project: (window.projectUI && window.projectUI.current()) || null,
      staging: false,
    },
    turbo: false,
  });
  $("faceModal").hidden = true;
  toast(`换${bodyMode ? "身" : "头"}完成：${fmtTime(res.seconds)}（参考：${card.name}）`);
  await loadLibrary();
  const list = sortedLibrary();
  const idx = list.findIndex((x) => x.path === res.images[0].path);
  if (idx >= 0) {
    viewer.list = list;
    viewer.idx = idx;
    await showViewerItem();
  }
}

async function runFaceSwap() {
  const it = viewer.list[viewer.idx];
  if (!it) return;
  const prompt = $("facePrompt").value.trim();
  if (!prompt) {
    toast("写一下这张脸要画成什么", true);
    return;
  }
  const btn = $("faceGo");
  btn.disabled = true;
  btn.textContent = faceIsQwen() ? "排队中…" : "换脸中…";
  try {
    if (faceIsQwen()) {
      await qwenRun(it, btn);
      return;
    }
    const target = faceSwapTarget();
    if (!target) {
      toast("没有能改图的模型：先在顶部切到 FLUX.3", true);
      return;
    }
    if (!faceSwap.bbox) {
      faceSwap.bbox = await faceSwapDetect();
      $("faceInfo").textContent = `${it.filename} · 找到脸`;
    }
    const res = await api.call("face_swap", {
      req: {
        source: it.path,
        face: faceSwap.bbox,
        prompt,
        negative: $("negative").value,
        profile: target.id,
        steps: Number($("faceSteps").value) || target.steps,
        cfg: target.cfg,
        seed: seedArg(),
        denoise: Number($("faceDenoise").value),
        feather: Number($("faceFeather").value) || 0,
        pad: Number($("facePad").value) || 1.6,
        refImageDataUrl: faceSwap.refDataUrl || null,
      },
    });
    $("faceModal").hidden = true;
    toast(`换脸完成：${fmtTime(res.seconds)}`);
    await loadLibrary();
    const list = sortedLibrary();
    const idx = list.findIndex((x) => x.path === res.images[0].path);
    if (idx >= 0) {
      viewer.list = list;
      viewer.idx = idx;
      await showViewerItem();
    }
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "开始";
  }
}

$("vwFace").onclick = () => openFaceSwap();
$("faceCancel").onclick = () => ($("faceModal").hidden = true);
$("faceGo").onclick = () => runFaceSwap();
$("faceUsePlate").onclick = () => faceSwapUsePlate();
$("faceMode").onchange = () => faceSyncMode();
buildStepPresets($("faceStepPresets"), $("faceSteps"), [8, 12, 16, 20, 25, 30, 36]);
