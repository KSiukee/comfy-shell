// 整话一致性体检：查的是「记录能不能对上」，不是「脸像不像」。
//
// 和挂卡/出图分开放（shotcast.js）：一个是动作、一个是判读，职责不同；也避开上帝文件闸。
// 四条判据：
//   ① 挂的角色卡还在不在；
//   ② 角色有没有身份底板（没底板 = 只注入文字，换景必飘）；
//   ③ 同一角色跨格用的是不是同一张底板（换了底板 = 跨格换人，这是机械可查的硬证据）；
//   ④ 有没有漏图 / 漏账（有图但没记 cast，说明是挂卡之前出的图）。
//
// 明确的边界：这套查不出「画出来是不是同一个人」，那要视觉模型逐张判。

function shotCheck(shots) {
  const findings = [];
  const byId = new Map(shotCastCards().map((c) => [c.id, c]));
  const noPlate = new Set();
  const gone = new Set();
  const plates = new Map();
  let withImage = 0;
  let casted = 0;

  for (const s of shots) {
    const tag = `第 ${s.index} 格`;
    if ((s.images || []).length) withImage++;
    const chars = s.characters || [];
    if (chars.length) casted++;
    for (const id of chars) {
      const c = byId.get(id);
      if (!c) {
        if (!gone.has(id)) {
          gone.add(id);
          findings.push({ level: "red", text: `${tag}挂着一个已经不在的角色卡（${id}）——重新挂一次。` });
        }
        continue;
      }
      if (!c.identityPlate && !noPlate.has(id)) {
        noPlate.add(id);
        findings.push({
          level: "yellow",
          text: `「${c.name}」还没有身份底板：这一话只注入了文字，换景时脸容易飘（角色卡里点「出底板」）。`,
        });
      }
    }
    const cast = s.cast || [];
    if ((s.images || []).length && chars.length && !cast.length) {
      findings.push({
        level: "yellow",
        text: `${tag}有图，但没记下出图时用的角色/底板（是挂卡之前出的图？重画一次就会记上）。`,
      });
    }
    for (const c of cast) {
      if (!c.id || !c.plate) continue;
      if (!plates.has(c.id)) plates.set(c.id, []);
      plates.get(c.id).push({ index: s.index, plate: c.plate });
    }
  }

  for (const [id, arr] of plates) {
    const uniq = [...new Set(arr.map((x) => x.plate))];
    if (uniq.length < 2) continue;
    const c = byId.get(id);
    findings.push({
      level: "red",
      text: `「${c ? c.name : id}」在第 ${arr.map((x) => x.index).join("、")} 格用的不是同一张身份底板（${uniq.length} 张）——跨格会变成两个人，统一底板后重画这些格。`,
    });
  }

  const noImg = shots.filter((s) => !(s.images || []).length).map((s) => s.index);
  if (noImg.length) {
    findings.push({ level: "yellow", text: `还没出图：第 ${noImg.join("、")} 格（「整话出图」会先补这些）。` });
  }

  for (const c of byId.values()) {
    const used = plates.has(c.id) || shots.some((s) => (s.characters || []).includes(c.id));
    if (used && c.graded) {
      findings.push({
        level: "info",
        text: `「${c.name}」上次一致性盲测 ${c.graded}（单角色 6 张的分数，只作参考）。`,
      });
    }
  }

  const red = findings.filter((f) => f.level === "red").length;
  const yellow = findings.filter((f) => f.level === "yellow").length;
  return { findings, stats: { shots: shots.length, withImage, casted, red, yellow } };
}

function shotCheckRender() {
  const box = $("shotsCheckBox");
  if (!box) return;
  if (!shotsCache.length) {
    box.hidden = true;
    return;
  }
  const { findings } = shotCheck(shotsCache);
  box.textContent = "";
  box.hidden = false;
  const head = document.createElement("div");
  head.className = "shot-check-head";
  head.textContent = findings.length
    ? `整话体检：${findings.length} 条。注意——这里只能证明「记录对得上」，证明不了「脸是同一个人」。`
    : "整话体检：没有发现机械问题。注意——这只说明记录对得上，不能证明脸是同一个人。";
  box.appendChild(head);
  const order = { red: 0, yellow: 1, info: 2 };
  for (const f of findings.slice().sort((a, b) => order[a.level] - order[b.level])) {
    const row = document.createElement("div");
    row.className = `shot-check-row ${f.level}`;
    row.textContent = `${f.level === "red" ? "🔴" : f.level === "yellow" ? "🟡" : "ℹ️"} ${f.text}`;
    box.appendChild(row);
  }
}

/// 每次刷新镜头列表都顺手算一遍摘要（纯前端、零成本）。
async function shotSummaryUpdate() {
  const box = $("shotsSummary");
  if (!box) return;
  if (!shotsCache.length) {
    box.textContent = "";
    return;
  }
  await shotCastLoadCards();
  const { stats } = shotCheck(shotsCache);
  box.textContent = `共 ${stats.shots} 格 · 有图 ${stats.withImage} · 挂卡 ${stats.casted} 格 · 🔴 ${stats.red} 🟡 ${stats.yellow}`;
  if ($("shotsCheckBox") && !$("shotsCheckBox").hidden) shotCheckRender();
}
