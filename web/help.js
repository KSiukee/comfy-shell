// 使用说明（应用内弹窗）：直接读 web/使用说明.md，用一个极小的 Markdown 渲染器画出来。
// 单一来源——改说明书只改那个 md，不用动代码。

let helpLoaded = false;

function helpEscape(s) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function helpInline(s) {
  return helpEscape(s)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (m, text, url) => {
      const safe = /^(https?:|\/|#|[\w./-]+\.md$)/i.test(url) ? url : "#";
      return `<a href="${safe}" target="_blank" rel="noreferrer">${text}</a>`;
    });
}

function helpMarkdown(md) {
  const lines = md.replace(/\r\n/g, "\n").split("\n");
  const out = [];
  let i = 0;
  let para = [];
  const flush = () => {
    if (para.length) {
      out.push(`<p>${helpInline(para.join(" "))}</p>`);
      para = [];
    }
  };
  while (i < lines.length) {
    const line = lines[i];
    if (/^\s*```/.test(line)) {
      flush();
      const code = [];
      i++;
      while (i < lines.length && !/^\s*```/.test(lines[i])) code.push(lines[i++]);
      i++;
      out.push(`<pre><code>${helpEscape(code.join("\n"))}</code></pre>`);
      continue;
    }
    if (/^\s*$/.test(line)) {
      flush();
      i++;
      continue;
    }
    if (/^---+$/.test(line.trim())) {
      flush();
      out.push("<hr />");
      i++;
      continue;
    }
    const head = /^(#{1,3})\s+(.*)$/.exec(line);
    if (head) {
      flush();
      const lv = head[1].length;
      out.push(`<h${lv}>${helpInline(head[2])}</h${lv}>`);
      i++;
      continue;
    }
    if (/^\s*>/.test(line)) {
      flush();
      const quote = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) quote.push(lines[i++].replace(/^\s*>\s?/, ""));
      out.push(`<blockquote>${helpInline(quote.join(" "))}</blockquote>`);
      continue;
    }
    if (/^\s*\|/.test(line)) {
      flush();
      const rows = [];
      while (i < lines.length && /^\s*\|/.test(lines[i])) rows.push(lines[i++]);
      const cells = (r) =>
        r
          .trim()
          .replace(/^\|/, "")
          .replace(/\|$/, "")
          .split("|")
          .map((c) => c.trim());
      let html = "<table>";
      rows.forEach((r, idx) => {
        if (/^\|[\s:|-]+\|?$/.test(r.trim())) return;
        const tag = idx === 0 ? "th" : "td";
        html += "<tr>" + cells(r).map((c) => `<${tag}>${helpInline(c)}</${tag}>`).join("") + "</tr>";
      });
      html += "</table>";
      out.push(html);
      continue;
    }
    if (/^\s*([-*]|\d+\.)\s+/.test(line)) {
      flush();
      const ordered = /^\s*\d+\./.test(line);
      const tag = ordered ? "ol" : "ul";
      const startAt = ordered ? parseInt(line.trim(), 10) || 1 : 1;
      let html = ordered && startAt > 1 ? `<${tag} start="${startAt}">` : `<${tag}>`;
      while (i < lines.length && /^\s*([-*]|\d+\.)\s+/.test(lines[i])) {
        html += `<li>${helpInline(lines[i].replace(/^\s*([-*]|\d+\.)\s+/, ""))}</li>`;
        i++;
      }
      html += `</${tag}>`;
      out.push(html);
      continue;
    }
    para.push(line.trim());
    i++;
  }
  flush();
  return out.join("\n");
}

async function openHelp() {
  const modal = $("helpModal");
  const body = $("helpBody");
  modal.hidden = false;
  if (helpLoaded) return;
  body.innerHTML = `<div class="muted">读取说明书…</div>`;
  try {
    const res = await fetch(encodeURI("使用说明.md"));
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const text = await res.text();
    body.innerHTML = helpMarkdown(text);
    helpLoaded = true;
    body.scrollTop = 0;
  } catch (e) {
    body.innerHTML = `<div class="muted">说明书没读到（web/使用说明.md）：${helpEscape(String(e))}</div>`;
  }
}

if ($("helpBtn")) $("helpBtn").onclick = openHelp;
if ($("helpClose")) $("helpClose").onclick = () => ($("helpModal").hidden = true);
if ($("helpModal")) {
  $("helpModal").addEventListener("click", (e) => {
    if (e.target === $("helpModal")) $("helpModal").hidden = true;
  });
}
