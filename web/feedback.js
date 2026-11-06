// 关于页：反馈发送（表单不通时退回复制 + 邮件客户端）与爱发电入口。
// 从 app.js 拆出来；行为不变。

// ---------------------------------------------------------------- 关于页：反馈 / 爱发电
async function sendFeedback() {
  const kind = $("fbKind") ? $("fbKind").value : "bug";
  const text = $("fbText") ? $("fbText").value.trim() : "";
  if (!text) return toast("先写点内容", true);
  const btn = $("fbSend");
  if (btn) btn.disabled = true;
  if ($("fbState")) $("fbState").textContent = "发送中…";
  try {
    await api.call("feedback_send", {
      kind,
      text,
      contact: $("fbContact") ? $("fbContact").value.trim() : "",
    });
    if ($("fbState")) $("fbState").textContent = "已发送，谢谢！";
    if ($("fbText")) $("fbText").value = "";
    toast("反馈已发送");
  } catch (e) {
    // 表单服务不通时兜底：复制内容 + 打开邮件客户端
    const ver = $("setAppVersion") ? $("setAppVersion").textContent : "";
    const body = `[${kind}] ${text}\n\n版本 ${ver}`;
    try {
      await navigator.clipboard.writeText(body);
    } catch {}
    const mailto =
      "mailto:ksiukee@qq.com?subject=" +
      encodeURIComponent("[显影反馈]") +
      "&body=" +
      encodeURIComponent(body);
    await api.call("open_url", { url: mailto }).catch(() => {});
    if ($("fbState")) $("fbState").textContent = "表单发送失败，已复制内容并打开邮件客户端";
    toast("已复制反馈内容，可直接粘贴发送", true);
  } finally {
    if (btn) btn.disabled = false;
  }
}
if ($("fbSend")) $("fbSend").onclick = sendFeedback;

if ($("afdianBtn")) {
  $("afdianBtn").onclick = () =>
    api.call("open_url", { url: "https://afdian.com/a/siukee" }).catch((e) => toast(String(e), true));
}
