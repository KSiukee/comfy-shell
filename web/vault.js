// 锁定区：口令加密的私密项目（Argon2id + XChaCha20-Poly1305）。
// 依赖 projects.js 暴露的 api / projCurrent / projRefresh / libScope 等全局。
// ---------------------------------------------------------------- 锁定区
let vaultState = { exists: false, unlocked: false };
let vaultIdleAt = Date.now();

function vEl(tag, text, cls) {
  const e = document.createElement(tag);
  if (text !== undefined) e.textContent = text;
  if (cls) e.className = cls;
  return e;
}

function vInput(placeholder) {
  const i = document.createElement("input");
  i.type = "password";
  i.placeholder = placeholder;
  i.style.cssText =
    "width:100%;background:#0f1219;color:var(--text);border:1px solid var(--line);border-radius:9px;padding:8px 10px;font-size:13px;margin-top:8px";
  return i;
}

async function vaultRefreshStatus() {
  try {
    vaultState = await api.call("vault_status");
  } catch {
    vaultState = { exists: false, unlocked: false };
  }
  const opt = document.querySelector('#libScope option[value="private"]');
  if (opt) {
    opt.disabled = !vaultState.unlocked;
    opt.textContent = vaultState.unlocked ? "私密" : "私密（锁定）";
  }
  if (libScope === "private" && !vaultState.unlocked) {
    libScope = "project";
    if (libScopeSel) libScopeSel.value = "project";
  }
  return vaultState;
}

async function vaultLockNow() {
  try {
    await api.call("vault_lock");
  } catch {}
  await vaultRefreshStatus();
  await renderVaultPane();
  projRefresh(projState.current);
  refreshLibraryIfOpen();
  toast("锁定区已锁定");
}

async function renderVaultPane() {
  const pane = $("vaultPane");
  if (!pane) return;
  pane.innerHTML = "";
  const hint = (t) => {
    const d = vEl("div", t, "hint");
    d.style.marginTop = "8px";
    pane.appendChild(d);
  };

  if (!vaultState.exists) {
    pane.appendChild(vEl("div", "第一次用：给锁定区设一个口令。私密项目的名字、配置、图片都会用它加密。"));
    const p1 = vInput("口令（至少 4 位）");
    const p2 = vInput("再输一次");
    pane.append(p1, p2);
    const b = vEl("button", "设置口令", "btn small primary");
    b.style.marginTop = "10px";
    b.onclick = async () => {
      if (p1.value.length < 4) return toast("口令至少 4 个字符", true);
      if (p1.value !== p2.value) return toast("两次口令不一样", true);
      try {
        await api.call("vault_setup", { pass: p1.value });
        await vaultRefreshStatus();
        await renderVaultPane();
        toast("锁定区已建立（口令丢了就打不开，记牢）");
      } catch (e) {
        toast(String(e), true);
      }
    };
    pane.appendChild(b);
    hint("口令无法找回；忘了只能删掉 vault 目录，等于丢数据。");
    return;
  }

  if (!vaultState.unlocked) {
    pane.appendChild(vEl("div", "锁定区已锁定，输口令解锁。"));
    const p = vInput("口令");
    pane.appendChild(p);
    const row = vEl("div", "", "row tight");
    row.style.marginTop = "10px";
    const b = vEl("button", "解锁", "btn small primary");
    b.onclick = async () => {
      try {
        await api.call("vault_unlock", { pass: p.value });
        await vaultRefreshStatus();
        await renderVaultPane();
        toast("已解锁");
      } catch {
        toast("口令不对", true);
      }
    };
    const l = vEl("button", "锁定", "btn small ghost");
    l.onclick = vaultLockNow;
    row.append(b, l);
    pane.appendChild(row);
    return;
  }

  const list = await api.call("project_private_list");
  if (!list.length) {
    pane.appendChild(vEl("div", "锁定区里还没有项目。"));
  }
  for (const p of list) {
    const row = vEl("div", "", "row tight");
    row.style.cssText = "border:1px solid var(--line);border-radius:8px;padding:8px;margin-top:8px";
    const name = vEl("span", p.name);
    name.style.flex = "1";
    const out = vEl("button", "移出隐私区", "btn small");
    out.onclick = async () => {
      try {
        const q = await api.call("project_unprivate", { id: p.id });
        await projRefresh(q.id);
        await renderVaultPane();
        refreshLibraryIfOpen();
        toast(`已移出：${q.name}`);
      } catch (e) {
        toast(String(e), true);
      }
    };
    row.append(name, out);
    pane.appendChild(row);
  }

  const actions = vEl("div", "", "row tight");
  actions.style.marginTop = "12px";
  const mk = vEl("button", "把当前项目移入锁定区", "btn small");
  mk.disabled = !projCurrent();
  mk.onclick = async () => {
    const cur = projCurrent();
    if (!cur) return;
    if (!confirm(`把「${cur.name}」移入锁定区？\n项目和图片会一起加密，之后只能解锁查看或移出。`)) return;
    try {
      await api.call("project_make_private", { id: cur.id });
      await projRefresh("");
      await renderVaultPane();
      refreshLibraryIfOpen();
      toast(`已移入锁定区：${cur.name}`);
    } catch (e) {
      toast(String(e), true);
    }
  };
  const lockBtn = vEl("button", "锁定", "btn small ghost");
  lockBtn.onclick = vaultLockNow;
  actions.append(mk, lockBtn);
  pane.appendChild(actions);
  hint("私密项目暂不支持直接生成；要改图先「移出隐私区」。");
}

$("vaultBtn").onclick = async () => {
  await vaultRefreshStatus();
  await renderVaultPane();
  $("vaultModal").hidden = false;
};
$("vaultClose").onclick = () => ($("vaultModal").hidden = true);

["pointerdown", "keydown"].forEach((ev) =>
  window.addEventListener(ev, () => { vaultIdleAt = Date.now(); }, true)
);
setInterval(async () => {
  if (!vaultState.unlocked) return;
  if (Date.now() - vaultIdleAt < 10 * 60 * 1000) return;
  await vaultLockNow();
}, 60 * 1000);

vaultRefreshStatus();
