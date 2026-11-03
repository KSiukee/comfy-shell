// 换脸（#60）：裁脸 → 整块重绘 → 羽化贴回。
//
// 和「局部重绘」的区别：这里不用画蒙版。前端只负责找脸（本机视觉模型给脸框），
// 裁块、整块重绘、羽化贴回都在后端一次做完（faceswap.rs / commands/faceswap.rs）——
// 只落一张结果图，图库里不会多出「一张裁出来的脸」。
//
// 为什么不再走蒙版换脸：椭圆蒙版横穿眉眼时模型只能在断口补画半套五官，
// 必然重影（docs/57 §8 的结案）。这里模型看到的是一整张脸。

const faceSwap = { bbox: null, refDataUrl: "" };

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
  $("faceInfo").textContent = `${it.filename} · 找脸中…`;
  $("faceRefState").textContent = "参考：未选（只靠提示词，样子不稳）";
  $("facePrompt").value = faceSwapDefaultPrompt();
  $("faceSteps").value = target.steps;
  $("faceDenoise").value = "0.75";
  $("faceFeather").value = "0";
  $("facePad").value = "1.6";
  $("faceGo").disabled = true;
  $("faceModal").hidden = false;
  try {
    const bbox = await faceSwapDetect();
    faceSwap.bbox = bbox;
    $("faceInfo").textContent = `${it.filename} · 找到脸（脸的宽高约 ${Math.round(bbox[2] * 100)}%×${Math.round(bbox[3] * 100)}%）`;
    $("faceGo").disabled = false;
  } catch (e) {
    $("faceInfo").textContent = `${it.filename} · 没找到脸`;
    $("faceGo").disabled = false;
    toast(`没找到脸：${e}（点「开始」会再试一次）`, true);
  }
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
  btn.textContent = "裁脸中…";
  try {
    faceSwap.refDataUrl = await inpaintPlateFaceDataUrl(card.identityPlate);
    $("faceRefState").textContent = `参考：${card.name} 的底板（已裁脸）`;
    toast(`已用「${card.name}」的底板裁出脸当参考`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = old;
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
  const target = faceSwapTarget();
  if (!target) {
    toast("没有能改图的模型：先在顶部切到 FLUX.3", true);
    return;
  }
  const btn = $("faceGo");
  btn.disabled = true;
  btn.textContent = "换脸中…";
  try {
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
buildStepPresets($("faceStepPresets"), $("faceSteps"), [12, 16, 20, 25, 30, 36]);
