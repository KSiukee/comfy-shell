proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- HTML ----------------
p = proj + r"\web\index.html"
h = open(p, encoding="utf-8").read()
h = h.replace('''          <button class="btn small ghost" id="btnAnalyze" title="丢一张图，让本地视觉模型拆成分类提示词">🔍 解析图</button>''',
'''          <button class="btn small ghost" id="btnAnalyze" title="丢一张图，让本地视觉模型拆成分类提示词">🔍 解析图</button>
          <button class="btn small ghost" id="btnLayout" title="MOSS 排版：拖框定位置，文字/物体分开标注" hidden>🎨 排版</button>''', 1)

h = h.replace('''      <div class="row tight">
        <button class="btn small primary" id="anaFillPrompt">组合成提示词</button>''',
'''      <div id="anaExtras" style="display:flex;flex-direction:column;gap:6px"></div>
      <div class="row tight">
        <button class="btn small ghost" id="anaAddExtra">＋ 加分类</button>
        <button class="btn small primary" id="anaFillPrompt">组合成提示词</button>''', 1)

layout_modal = '''
  <!-- MOSS layout editor -->
  <div class="modal" id="layoutModal" hidden>
    <div class="modal-card" style="width: 1000px">
      <h3>MOSS 排版 <span class="muted" style="font-size:12px">拖框定位置 · 文字/物体分开标注</span></h3>
      <div class="row tight">
        <input id="layoutSummary" placeholder="整张图的描述（summary）" style="flex: 2 1 0" />
        <button class="btn small" id="layoutAddText">＋ 文字</button>
        <button class="btn small" id="layoutAddObj">＋ 物体</button>
        <span class="muted mono" id="layoutHint"></span>
        <span class="spacer"></span>
        <button class="btn ghost small" id="layoutClose">关闭</button>
      </div>
      <div class="row tight" style="align-items:flex-start">
        <div id="layoutStage" style="position:relative;width:512px;height:512px;background:#0f1116;border:1px solid #2a2f3a;border-radius:8px;flex:0 0 auto;overflow:hidden">
          <div id="layoutBoxes"></div>
        </div>
        <div id="layoutList" style="flex:1;display:flex;flex-direction:column;gap:6px;max-height:512px;overflow:auto"></div>
      </div>
      <div class="row tight">
        <button class="btn small primary" id="layoutRun">按排版生成</button>
        <span class="muted" style="font-size:11.5px">生成时用 layout JSON 作为提示词，位置按当前宽高换算</span>
      </div>
    </div>
  </div>
'''
h = h.replace("  <!-- image analyzer -->", layout_modal + "\n  <!-- image analyzer -->", 1)
open(p, "w", encoding="utf-8", newline="\n").write(h)
print("html ok")

# ---------------- CSS ----------------
p = proj + r"\web\style.css"
c = open(p, encoding="utf-8").read()
c += '''
#layoutBoxes .lbox { position: absolute; border: 2px solid #5d7bf0; background: #5d7bf022; border-radius: 4px; cursor: move; }
#layoutBoxes .lbox.sel { border-color: #63d19e; background: #63d19e26; }
#layoutBoxes .lbox span { position: absolute; left: 2px; top: 2px; font-size: 10.5px; color: #dfe6f3; background: #0009; padding: 0 4px; border-radius: 3px; }
#layoutList .lrow { display: grid; grid-template-columns: 40px 1fr 28px; gap: 6px; align-items: center; }
#layoutList .lrow input { font-size: 12px; }
#layoutList .lrow .tag { font-size: 11px; color: #8e97a8; }
#layoutList .lrow button { padding: 0 6px; }
#anaExtras .xrow { display: grid; grid-template-columns: 90px 1fr 28px; gap: 8px; }
#anaExtras input { font-size: 12.5px; }
'''
open(p, "w", encoding="utf-8", newline="\n").write(c)
print("css ok")
