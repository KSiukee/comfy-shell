// 角色一致性盲测：跑 3 个场景 × 2 张，逐张判「认得出 / 认不出」，分数记进角色卡。
//
// 单独成文件的原因：characters.js 已经贴到 400 行（上帝文件闸），盲测这块是独立职责。

const CHAR_TEST_SCENES = [
  "室内窗边，半身，柔和窗光，浅景深",
  "雨夜街头，全身，霓虹倒影，侧光",
  "黄昏逆光，特写，暖色调",
];
const CHAR_TEST_EACH = 2;

let charTestShots = []; // [{scene, url, path, verdict: "" | "ok" | "no"}]

function charTestRender() {
  const grid = $("charTestGrid");
  grid.textContent = "";
  let ok = 0;
  charTestShots.forEach((shot) => {
    if (shot.verdict === "ok") ok++;
    const cell = document.createElement("div");
    cell.className = "char-test-cell" + (shot.verdict === "ok" ? " ok" : shot.verdict === "no" ? " no" : "");
    const img = document.createElement("img");
    img.src = shot.url;
    img.alt = shot.scene;
    const tag = document.createElement("span");
    tag.textContent = `${shot.scene} · ${shot.verdict === "ok" ? "认得出" : shot.verdict === "no" ? "认不出" : "点一下判断"}`;
    cell.append(img, tag);
    cell.onclick = () => {
      shot.verdict = shot.verdict === "ok" ? "no" : "ok";
      charTestRender();
    };
    grid.appendChild(cell);
  });
  $("charTestCount").textContent = `认得出 ${ok} / ${charTestShots.length}`;
}

/// 跑 3 场景 × 2 张，走真实的 @角色 通路。
async function charRunTest() {
  const c = charCollect();
  if (!c.name) {
    toast("先填角色名", true);
    return;
  }
  if (!c.identityPlate) {
    toast("先出底板再做一致性测试", true);
    return;
  }
  const btn = $("charTestBtn");
  btn.disabled = true;
  const oldPrompt = $("prompt").value;
  const total = CHAR_TEST_SCENES.length * CHAR_TEST_EACH;
  charTestShots = [];
  try {
    let n = 0;
    for (const scene of CHAR_TEST_SCENES) {
      for (let k = 0; k < CHAR_TEST_EACH; k++) {
        n++;
        btn.textContent = `生成 ${n}/${total}…`;
        $("prompt").value = `${scene}，@${c.name}`;
        const ok = await generate();
        const first = (state.results || [])[0];
        if (ok && first) {
          charTestShots.push({ scene: scene.split("，")[0], url: first.dataUrl, path: first.path, verdict: "" });
        }
      }
    }
  } catch (e) {
    toast(`一致性测试出错：${e}`, true);
  } finally {
    $("prompt").value = oldPrompt;
    btn.disabled = false;
    btn.textContent = "一致性测试";
  }
  if (!charTestShots.length) {
    toast("一张都没生成出来", true);
    return;
  }
  charTestRender();
  $("charModal").hidden = true; // 别让角色卡挡住盲测窗
  $("charTestModal").hidden = false;
}

async function charTestSave() {
  const ok = charTestShots.filter((s) => s.verdict === "ok").length;
  const total = charTestShots.length;
  if (!total) return;
  const c = charCollect();
  const d = new Date();
  const stamp = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  c.graded = `${stamp} ${ok}/${total}`;
  try {
    const saved = await api.call("character_save", { character: c });
    charCurrent = saved;
    await charRefresh(saved.id);
    $("charTestModal").hidden = true;
    $("charModal").hidden = false;
    toast(`已记录：${ok}/${total}`);
  } catch (e) {
    toast(String(e), true);
  }
}

$("charTestBtn").onclick = () => charRunTest();
$("charTestClose").onclick = () => {
  $("charTestModal").hidden = true;
  $("charModal").hidden = false;
};
$("charTestSave").onclick = () => charTestSave();
