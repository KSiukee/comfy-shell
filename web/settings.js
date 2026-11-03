// 设置弹窗的读写（#86 起从 app.js 搬出来：那边只许缩，不许长）。
//
// 规矩没变：**从现有设置铺开再覆盖表单上的字段**——别把 simpleUi / kohya 路径
// 之类的旧字段冲掉，否则保存一次就把它们重置成默认值。

function collectSettings() {
  const prevOut = (state.settings && state.settings.outputDir) || "";
  return Object.assign({}, state.settings || {}, {
    comfyRoot: $("setRoot").value.trim(),
    port: Number($("setPort").value),
    outputDir: $("setOutputDir") ? $("setOutputDir").value.trim() : prevOut,
    autoStart: $("setAutoStart").checked,
    stopOnExit: $("setStopOnExit").checked,
    freeEnhance: $("setFreeEnhance").checked,
    // #86 超分后替换原图（默认关）
    replaceOriginal: $("setReplaceOriginal") ? $("setReplaceOriginal").checked : false,
  });
}

async function saveSettings(closeModal) {
  const next = collectSettings();
  const prevOut = (state.settings && state.settings.outputDir) || "";
  await api.call("settings_set", { settings: next });
  state.settings = await api.call("settings_get");
  if (closeModal) $("settingsModal").hidden = true;
  if (next.outputDir !== prevOut) toast("出图目录已保存：重启 ComfyUI 后生效");
  else if (closeModal) toast("已保存");
  refreshStatus();
}
