// 训练 UI（从 app.js 拆出；依赖 app.js 的全局）

const trainState = { src: "library", files: [], busy: false };

async function trainingOpen() {
  const picked = libItems.filter((i) => libSel.has(i.path));
  trainState.files = [];
  $("trainFolder").value = "";
  $("trainResult").hidden = true;
  $("trainLoss").removeAttribute("src");
  $("trainGo").hidden = false;
  $("trainCancel").textContent = "取消";
  $("trainBar").style.width = "0%";
  $("trainProgressText").textContent = "";
  document.querySelector('input[name="trainSrc"][value="library"]').checked = true;
  trainState.src = "library";
  const ex = $("trainExisting");
  ex.innerHTML = `<option value="">（从零开始）</option>` +
    (state.models.loras || []).map((l) => `<option value="${l}">${l}</option>`).join("");
  ex.value = "";
  $("trainCount").textContent = `${picked.length} 张`;
  $("trainHint").textContent =
    "图库模式用每张图的生成提示词当 caption；文件夹模式自动缩放到所选尺寸（中心裁切或等比镜像填充）并用下面的 caption。训练需壳托管的 ComfyUI（已关 pinned memory）。";
  if (!$("trainName").value) {
    $("trainName").value = "lora_" + Date.now().toString(36).slice(-4);
  }
  $("trainModal").hidden = false;
  $("trainName").focus();
}

const trainAnn = [];

function buildAnnRows() {
  const box = $("annList");
  if (!box) return;
  box.innerHTML = "";
  trainAnn.length = 0;
  if (trainState.src !== "folder" || !trainState.files.length) {
    $("annRow").hidden = true;
    return;
  }
  $("annRow").hidden = false;
  trainState.files.forEach((f, i) => {
    const row = document.createElement("div");
    row.className = "ann-row";
    const img = document.createElement("img");
    img.src = URL.createObjectURL(f);
    const want = document.createElement("input");
    want.placeholder = "想要：如 red coat, silver hair";
    const avoid = document.createElement("input");
    avoid.placeholder = "不要：如 background clutter";
    const weight = document.createElement("select");
    weight.innerHTML = `<option value="1">×1</option><option value="2">×2</option><option value="3">×3</option>`;
    const cropBtn = document.createElement("button");
    cropBtn.type = "button";
    cropBtn.className = "btn ghost small";
    cropBtn.textContent = "框";
    cropBtn.title = "在图上标绿（保留）/ 红（抹掉）";
    const entry = { want, avoid, weight, crop: null, erases: [], cropBtn };
    cropBtn.onclick = () => openMark(f, entry);
    img.style.cursor = "zoom-in";
    img.onclick = () => openMark(f, entry);
    row.append(img, want, avoid, weight, cropBtn);
    box.appendChild(row);
    trainAnn.push(entry);
  });
}

function trainRefreshSource() {
  buildAnnRows();
  const folder = trainState.src === "folder";
  $("trainCaptionRow").hidden = !folder;
  $("trainCount").textContent = folder
    ? `${trainState.files.length} 张`
    : `${libItems.filter((i) => libSel.has(i.path)).length} 张`;
}

async function readFileDataUrl(f) {
  return new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () => res(r.result);
    r.onerror = rej;
    r.readAsDataURL(f);
  });
}

const training = { busy: false };

async function trainingStart() {
  if (trainState.busy) return;
  const simple = document.body.classList.contains("mode-simple");
  if (simple) {
    const k = simpleTrainKnobs();
    $("trainSize").value = String(k.size);
    $("trainRank").value = String(k.rank);
    $("trainSteps").value = String(k.steps);
    const pid = profile()?.id || "flux3base";
    $("trainBase").value = pid.startsWith("moss") ? "moss" : "flux3base";
    $("trainEval").value = "0";
    $("trainRestart").checked = true;
  }
  const picked = libItems.filter((i) => libSel.has(i.path));
  const name = $("trainName").value.trim();
  const base = $("trainBase").value;
  if (!/^[A-Za-z0-9_-]{2,40}$/.test(name)) {
    toast("名字只能用字母/数字/下划线，2-40 字", true);
    return;
  }
  if (trainState.src === "library" && !picked.length) {
    toast("先勾选训练用的图", true);
    return;
  }
  if (trainState.src === "folder" && !trainState.files.length) {
    toast("先选一个文件夹", true);
    return;
  }
  trainState.busy = true;
  $("trainGo").disabled = true;
  $("trainGo").textContent = "训练中…";
  $("trainBar").style.width = "0%";
  $("trainProgressText").textContent = "准备数据…";
  const t0 = Date.now();
  const timer = setInterval(() => {
    if (!$("trainProgressText").textContent.includes("/")) {
      $("trainProgressText").textContent = `训练中 ${Math.round((Date.now() - t0) / 1000)}s`;
    }
  }, 1000);
  try {
    let res;
    const engine = $("trainEngine") ? $("trainEngine").value : "auto";
    const useKohya = engine === "kohya" || (engine === "auto" && base.startsWith("moss"));
    if (useKohya && trainState.src !== "folder") {
      toast("kohya 引擎只支持「外部文件夹」数据源", true);
      return;
    }
    if (trainState.src === "folder") {
      const files = [];
      for (const f of trainState.files) {
        files.push({ name: f.name, dataUrl: await readFileDataUrl(f) });
      }
      const filesPayload = {
        name,
        base,
        steps: Number($("trainSteps").value),
        rank: Number($("trainRank").value),
        lr: Number($("trainLr").value),
        files,
        caption: $("trainCaption").value,
        autoCaption: $("trainAutoCaption").checked,
        existingLora: $("trainExisting").value || null,
        size: Number($("trainSize").value),
        keepAspect: $("trainAspect").value === "keep",
        annotations: trainAnn.map((a) => ({
          want: a.want.value,
          avoid: a.avoid.value,
          weight: Number(a.weight.value),
          crop: a.crop || null,
          erases: a.erases || [],
        })),
        evalPrompts: Number($("trainEval").value) || 0,
        batch: Number($("trainBatch").value) || 1,
        gradAccum: Number($("trainGrad").value) || 1,
        depth: Number($("trainDepth").value) || 3,
        offload: $("trainOffload").checked,
        dtype: $("trainDtype").value,
        restart: $("trainRestart").checked,
      };
      res = await api.call(useKohya ? "train_kohya_files" : "train_lora_files", { req: filesPayload });
      res = await api.call("train_lora", {
        req: {
          name,
          base,
          steps: Number($("trainSteps").value),
          rank: Number($("trainRank").value),
          lr: Number($("trainLr").value),
          images: picked.map((p) => p.path),
          captions: picked.map((p) => libPrompts.get(p.path) || ""),
          existingLora: $("trainExisting").value || null,
          size: Number($("trainSize").value),
          keepAspect: $("trainAspect").value === "keep",
          annotations: trainAnn.map((a) => ({
            want: a.want.value,
            avoid: a.avoid.value,
            weight: Number(a.weight.value),
            crop: a.crop || null,
            erases: a.erases || [],
          })),
          evalPrompts: Number($("trainEval").value) || 0,
          batch: Number($("trainBatch").value) || 1,
          gradAccum: Number($("trainGrad").value) || 1,
          depth: Number($("trainDepth").value) || 3,
          offload: $("trainOffload").checked,
          dtype: $("trainDtype").value,
          restart: $("trainRestart").checked,
        },
      });
    }
    // register in the production list and switch to it
    state.models = await api.call("models_list");
    const loraSel = $("lora");
    loraSel.innerHTML =
      `<option value="">（无）</option>` +
      state.models.loras.map((l) => `<option value="${l}">${l}</option>`).join("");
    if (![...loraSel.options].some((o) => o.value === res.lora)) {
      const opt = document.createElement("option");
      opt.value = res.lora;
      opt.textContent = res.lora;
      loraSel.appendChild(opt);
    }
    loraSel.value = res.lora;
    $("trainBar").style.width = "100%";
    $("trainProgressText").textContent = `完成 · ${res.lora}`;
    if (res.lossImage) {
      $("trainLoss").src = res.lossImage;
      $("trainResult").hidden = false;
    }
    const grid = $("trainEvalGrid");
    grid.innerHTML = "";
    const shots = res.evalImages || [];
    $("trainEvalWrap").hidden = shots.length === 0;
    for (const path of shots) {
      const img = document.createElement("img");
      img.alt = "eval";
      grid.appendChild(img);
      api.call("read_output_image", { path }).then((url) => (img.src = url)).catch(() => {});
    }
    if (res.avoid) toast(`建议负面：${res.avoid}`);
    $("trainGo").hidden = true;
    $("trainCancel").textContent = "关闭";
    toast(`训练完成：${res.lora}（已切到该 LoRA）`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    clearInterval(timer);
    trainState.busy = false;
    $("trainGo").disabled = false;
    $("trainGo").textContent = "开始训练";
  }
}

$("libBatchUp").onclick = batchUpscale;
$("libExternal").onclick = () => $("extFolder").click();
$("extFolder").onchange = async (e) => {
  const files = e.target.files;
  e.target.value = "";
  if (files && files.length) await externalUpscale(files);
};
$("libTrain").onclick = trainingOpen;
$("trainExisting").onchange = () => {
  const v = $("trainExisting").value;
  if (v) {
    const base = v.replace(/\.safetensors$/i, "");
    $("trainName").value = `${base}_v2`;
  }
};
$("trainBase").onchange = () => {
  if ($("trainBase").value === "moss") {
    $("trainHint").textContent =
      "MOSS bf16（9.5GB）在本机 16G 卡上实测会 OOM：开 offload 报 aimdo 错误，不开直接崩。建议用 FLUX.3 Base；MOSS 需要 24G 以上。";
  } else {
    $("trainHint").textContent =
      "图库模式用每张图的生成提示词当 caption；文件夹模式自动缩放到所选尺寸（中心裁切或等比镜像填充）并用下面的 caption。训练需壳托管的 ComfyUI（已关 pinned memory）。";
  }
};
$("trainCancel").onclick = () => ($("trainModal").hidden = true);
$("trainGo").onclick = trainingStart;


for (const r of document.querySelectorAll('input[name="trainSrc"]')) {
  r.onchange = () => {
    trainState.src = r.value;
    trainRefreshSource();
  };
}
$("trainFolderPick").onclick = () => $("trainFolder").click();
$("trainFolder").onchange = (e) => {
  trainState.files = [...e.target.files].filter((f) => /\.(png|jpe?g|webp)$/i.test(f.name));
  if (trainState.src !== "folder") {
    document.querySelector('input[name="trainSrc"][value="folder"]').checked = true;
    trainState.src = "folder";
  }
  trainRefreshSource();
};
listen("caption-progress", (e) => {
  const { done, total } = e.payload || {};
  if (!total) return;
  $("trainProgressText").textContent = `自动 caption ${done} / ${total}`;
}).catch(() => {});

listen("train-progress", (e) => {
  const { value, max } = e.payload || {};
  if (!max) return;
  $("trainBar").style.width = `${Math.min(100, (value / max) * 100)}%`;
  $("trainProgressText").textContent = `${value} / ${max} 步`;
}).catch(() => {});

// quick step presets (chips) for the main panel and the inpaint dialog
const STEP_PRESETS = [30, 36, 42, 49, 25, 50];

function syncStepPresets(container, input, values) {
  const current = Number(input.value);
  for (const b of container.querySelectorAll("button")) {
    b.classList.toggle("on", Number(b.dataset.steps) === current);
  }
}

function buildStepPresets(container, input, values = STEP_PRESETS) {
  container.innerHTML = "";
  for (const v of values) {
    const b = document.createElement("button");
    b.type = "button";
    b.dataset.steps = String(v);
    b.textContent = String(v);
    b.onclick = () => {
      input.value = String(v);
      input.dispatchEvent(new Event("input", { bubbles: true }));
      syncStepPresets(container, input);
    };
    container.appendChild(b);
  }
  input.addEventListener("input", () => syncStepPresets(container, input));
  syncStepPresets(container, input);
}

buildStepPresets($("stepPresets"), $("steps"));
buildStepPresets($("inpaintStepPresets"), $("inpaintSteps"), [20, 25, 30, 36, 42, 49]);

// training resource presets: size / rank / depth / steps / offload
const TRAIN_PRESETS = [
  { label: "低配 384", size: 384, rank: 8, depth: 3, steps: 120, offload: true },
  { label: "标准 512", size: 512, rank: 16, depth: 3, steps: 200, offload: true },
  { label: "细节 640", size: 640, rank: 16, depth: 3, steps: 200, offload: true },
];
(() => {
  const box = $("trainPresets");
  if (!box) return;
  for (const p of TRAIN_PRESETS) {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = p.label;
    b.onclick = () => {
      $("trainSize").value = String(p.size);
      $("trainRank").value = String(p.rank);
      $("trainDepth").value = String(p.depth);
      if ($("trainRestart")) $("trainRestart").checked = true;
      $("trainSteps").value = String(p.steps);
      $("trainOffload").checked = p.offload;
      toast(`已套用 ${p.label}`);
    };
    box.appendChild(b);
  }
})();


// ---------------------------------------------------------------- portrait presets
