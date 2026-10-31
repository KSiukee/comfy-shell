// 生成图 PNG 元数据：解析 + 拖回恢复参数。
// 依赖 app.js 的 state/$/api/toast/setMode 与 projectUI。

function parsePngMeta(bytes) {
  if (!bytes || bytes.length < 12) return null;
  const sig = [137, 80, 78, 71, 13, 10, 26, 10];
  for (let i = 0; i < 8; i++) if (bytes[i] !== sig[i]) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const dec = new TextDecoder();
  let off = 8;
  while (off + 8 <= bytes.length) {
    const len = view.getUint32(off);
    if (off + 12 + len > bytes.length) return null;
    const type = String.fromCharCode(...bytes.subarray(off + 4, off + 8));
    const data = bytes.subarray(off + 8, off + 8 + len);
    if (type === "iTXt" || type === "tEXt") {
      const keyEnd = data.indexOf(0);
      if (keyEnd > 0) {
        const key = dec.decode(data.subarray(0, keyEnd));
        if (key === "comfyshell") {
          if (type === "tEXt") {
            try {
              return JSON.parse(dec.decode(data.subarray(keyEnd + 1)));
            } catch {
              return null;
            }
          }
          let p = keyEnd + 1;
          if (p + 2 > data.length) return null;
          const flag = data[p];
          p += 2;
          const langEnd = data.indexOf(0, p);
          if (langEnd < 0) return null;
          p = langEnd + 1;
          const transEnd = data.indexOf(0, p);
          if (transEnd < 0) return null;
          p = transEnd + 1;
          if (flag !== 0) return null;
          try {
            return JSON.parse(dec.decode(data.subarray(p)));
          } catch {
            return null;
          }
        }
      }
    }
    if (type === "IEND") break;
    off += 12 + len;
  }
  return null;
}

async function readPngMetaFile(file) {
  try {
    const buf = await file.arrayBuffer();
    return parsePngMeta(new Uint8Array(buf));
  } catch {
    return null;
  }
}

function setVal(id, v) {
  const el = $(id);
  if (el !== null && el !== undefined && v !== undefined && v !== null) el.value = v;
}

async function applyImageMeta(meta) {
  if (!meta || meta.app !== "comfy-shell") return false;
  if (meta.project) {
    try {
      const ps = await api.call("project_list", { includeHidden: true });
      if (ps.some((p) => p.id === meta.project)) {
        await api.call("project_set_active", { id: meta.project });
        await projectUI.refresh(meta.project);
      }
    } catch {}
  }
  setVal("prompt", meta.prompt || "");
  setVal("negative", meta.negative || "");
  setVal("width", meta.width);
  setVal("height", meta.height);
  setVal("steps", meta.steps);
  setVal("cfg", meta.cfg);
  setVal("seed", meta.seed);
  setVal("batch", 1);
  if (meta.profile) {
    const sel = $("model");
    if ([...sel.options].some((o) => o.value === meta.profile)) sel.value = meta.profile;
  }
  if (meta.lora !== undefined) {
    const sel = $("lora");
    if ([...sel.options].some((o) => o.value === (meta.lora || ""))) sel.value = meta.lora || "";
  }
  if (meta.loraStrength !== undefined) {
    $("loraStrength").value = meta.loraStrength;
    $("loraStrengthText").textContent = Number(meta.loraStrength).toFixed(2);
  }
  if (meta.lora2 !== undefined) {
    const sel = $("lora2");
    if ([...sel.options].some((o) => o.value === (meta.lora2 || ""))) sel.value = meta.lora2 || "";
  }
  if (meta.lora2Strength !== undefined) {
    $("lora2Strength").value = meta.lora2Strength;
    $("lora2StrengthText").textContent = Number(meta.lora2Strength).toFixed(2);
  }
  if (meta.slg !== undefined) {
    $("mossSlg").checked = !!meta.slg;
    state.slgTouched = true;
  }
  if (meta.slgLayers) setVal("slgTier", meta.slgLayers);
  state.pendingLayout = meta.layout || null;
  if (typeof setMode === "function") setMode("t2i");
  toast(`已恢复参数${meta.project ? " · 项目已切换" : ""}`);
  return true;
}

window.addEventListener("dragover", (e) => {
  if (e.dataTransfer && [...e.dataTransfer.types].includes("Files")) e.preventDefault();
});

window.addEventListener("drop", async (e) => {
  if (!e.dataTransfer) return;
  const target = e.target instanceof Element ? e.target : null;
  if (target && target.closest("#dropzone")) return; // 参考图 dropzone 自己处理
  const files = [...e.dataTransfer.files];
  if (!files.length) return;
  e.preventDefault();
  for (const f of files) {
    if (!/\.png$/i.test(f.name)) continue;
    const meta = await readPngMetaFile(f);
    if (meta) {
      await applyImageMeta(meta);
      return;
    }
  }
  toast("这张 PNG 没有显影参数（不是本壳生成的图？）", true);
});
