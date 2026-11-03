// 幻灯片播放（#85）：图区 / 图库共用一套。
//
// 空格 = 开始/暂停，→ = 下一张（图区本来就有），Esc = 停；间隔 2/3/5 秒。
// 图库播放跟着**当前筛选与排序**走：点「播放」时把 sortedLibrary() 的结果交给大图，
// 从选中的那张开始，所以「只看收藏 / 搜索 / 按文件名」都会影响播放列表。
//
// 实现上只有一个 setTimeout 链：每放完一张再排下一张（改间隔立刻生效，不会半路跳拍）。

const slide = { on: false, timer: 0, gapMs: 3000, starOnly: false };
const SLIDE_GAP_KEY = "cs.slideGap";
const SLIDE_STAR_KEY = "cs.slideStarOnly";

function slideGapMs() {
  const sel = $("vwPlayGap");
  const v = sel ? Number(sel.value) : 3000;
  return Number.isFinite(v) && v >= 1000 ? v : 3000;
}

function slideLoadGap() {
  try {
    const saved = Number(localStorage.getItem(SLIDE_GAP_KEY));
    if ([2000, 3000, 5000].includes(saved) && $("vwPlayGap")) $("vwPlayGap").value = String(saved);
    slide.starOnly = localStorage.getItem(SLIDE_STAR_KEY) === "1";
  } catch {}
}

function slideSaveGap() {
  try {
    localStorage.setItem(SLIDE_GAP_KEY, String(slideGapMs()));
  } catch {}
}

function slideRender() {
  const btn = $("vwPlay");
  if (!btn) return;
  btn.textContent = slide.on ? "⏸" : "▶";
  btn.classList.toggle("primary", slide.on);
  btn.title = slide.on ? "暂停播放（空格）" : "幻灯片播放（空格开始，Esc 停）";
  const star = $("vwPlayStar");
  if (star) {
    star.classList.toggle("primary", slide.starOnly);
    star.title = slide.starOnly
      ? "正在只播放收藏（点一下改回全部）"
      : "只播放收藏的图";
  }
}

/// 「只放收藏」时的播放列表：当前筛选/排序 + 只要收藏过的。
function slideStarList() {
  return sortedLibrary().filter((i) => i.starred);
}

function slideSetStarOnly(on) {
  slide.starOnly = !!on;
  try {
    localStorage.setItem(SLIDE_STAR_KEY, slide.starOnly ? "1" : "0");
  } catch {}
  slideRender();
  if (slide.on) {
    // 正在放：按新范围重开，别让「只放收藏」等下一次才生效
    slideStop();
    slideStart();
  }
}

function slideClear() {
  if (slide.timer) {
    clearTimeout(slide.timer);
    slide.timer = 0;
  }
}

function slideStop(msg) {
  slideClear();
  const was = slide.on;
  slide.on = false;
  slideRender();
  if (msg && was) toast(msg);
}

function slideSchedule() {
  slideClear();
  if (!slide.on) return;
  slide.timer = setTimeout(slideTick, slideGapMs());
}

async function slideTick() {
  slide.timer = 0;
  if (!slide.on) return;
  if ($("viewer").hidden) return slideStop();
  if (typeof maskState !== "undefined" && maskState.active) return slideStop();
  if (viewer.idx >= viewer.list.length - 1) return slideStop("播放结束");
  viewer.idx += 1;
  await showViewerItem();
  slideSchedule();
}

/// 用户手动翻页（按 → / 点箭头 / 滚轮）时重新计时：否则刚翻过去就又被定时器顶一张。
function slideNoteStep() {
  if (slide.on) slideSchedule();
}

async function slideStart() {
  // 「只放收藏」：播放列表换成收藏过的（仍然按当前筛选与排序），从当前这张接上
  if (slide.starOnly) {
    // 图库没打开过时 libItems 还是空的——先扫一次，别让人在「收藏夹是空的」上撞墙
    if (!libItems.length && typeof loadLibrary === "function") await loadLibrary();
    const list = slideStarList();
    if (!list.length) {
      toast("收藏夹是空的，没法只放收藏", true);
      return;
    }
    const cur = viewer.list[viewer.idx];
    let idx = cur ? list.findIndex((x) => x.path === cur.path) : -1;
    if (idx < 0) idx = 0;
    viewer.list = list;
    viewer.idx = idx;
    showViewerItem();
  }
  if (!viewer.list.length) {
    toast("没有可播放的图", true);
    return;
  }
  // 已经在最后一张：从第一张重新放，否则一按就「播放结束」
  if (viewer.list.length > 1 && viewer.idx >= viewer.list.length - 1) {
    viewer.idx = 0;
    showViewerItem();
  }
  slide.on = true;
  slideRender();
  slideSchedule();
}

function slideToggle() {
  if (slide.on) slideStop();
  else slideStart();
}

/// 图库播放：把「当前筛选 + 排序」的整列交给大图，从选中的一张开始。
async function libPlay() {
  const list = sortedLibrary();
  if (!list.length) {
    toast("没有可播放的图", true);
    return;
  }
  let idx = 0;
  if (libSel.size === 1) {
    const p = [...libSel][0];
    const i = list.findIndex((x) => x.path === p);
    if (i >= 0) idx = i;
  }
  await openViewer(list, idx);
  slideStart();
}

// 键盘：空格开/关，播放中 Esc 只停不关大图。
// 用**捕获阶段**拦：app.js 里那套大图快捷键是冒泡阶段，这里先一步处理并把事件截住，
// 省得为了两个键去改 app.js（那个文件只许缩，不许长）。
window.addEventListener(
  "keydown",
  (e) => {
    if ($("viewer").hidden) return;
    const t = e.target;
    if (t && t.tagName && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
    if (e.key === " " || e.code === "Space") {
      e.preventDefault();
      e.stopPropagation();
      slideToggle();
      return;
    }
    if (e.key === "Escape" && slide.on) {
      e.preventDefault();
      e.stopPropagation();
      slideStop();
    }
  },
  true,
);

if ($("vwPlay")) $("vwPlay").onclick = () => slideToggle();
if ($("vwPlayStar")) $("vwPlayStar").onclick = () => slideSetStarOnly(!slide.starOnly);
if ($("vwPlayGap")) {
  $("vwPlayGap").onchange = () => {
    slideSaveGap();
    if (slide.on) slideSchedule();
  };
}
if ($("libPlay")) $("libPlay").onclick = () => libPlay();
slideLoadGap();
slideRender();
