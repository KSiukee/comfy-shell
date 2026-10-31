// 备份 / 导出 / 导入（设置里的“数据”按钮）。
// 私密项目导出会解密成明文：这里先明确确认，manifest 里也会标 plaintext。
async function pickFolder() {
  return await api.call("pick_folder");
}

$("btnBackup").onclick = async () => {
  const dest = await pickFolder();
  if (!dest) return;
  try {
    const dir = await api.call("backup_now", { dest });
    toast(`已备份：${dir}`);
    await api.call("open_folder", { path: dir });
  } catch (e) {
    toast(String(e), true);
  }
};

$("btnExportProject").onclick = async () => {
  const p = window.projectUI && window.projectUI.currentObj ? window.projectUI.currentObj() : null;
  if (!p) return toast("先选一个项目", true);
  if (p.private && !confirm("这是私密项目：导出会把内容解密成明文。\n确定导出到明文文件夹？")) return;
  const warn = typeof poseLicenseWarn === "function" ? poseLicenseWarn() : "";
  if (warn && !confirm(`本项目用过仅本地使用的姿势库：\n${warn}\n\n继续导出？`)) return;
  const dest = await pickFolder();
  if (!dest) return;
  try {
    const dir = await api.call("project_export", { id: p.id, dest });
    toast(`已导出：${dir}`);
    await api.call("open_folder", { path: dir });
  } catch (e) {
    toast(String(e), true);
  }
};

$("btnImportProject").onclick = async () => {
  const folder = await pickFolder();
  if (!folder) return;
  try {
    const p = await api.call("project_import", { folder });
    await projectUI.refresh(p.id);
    toast(`已导入：${p.name}`);
  } catch (e) {
    toast(String(e), true);
  }
};
