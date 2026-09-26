// 判定「窗口在内容渲染之前被显示」是谁干的：
// 在页面文档创建前注入探针，记录 DOM 里程碑、data-theme 属性写入、所有 IPC 调用、
// 以及窗口可见性变化——全部在页面同一条时钟上，无需跨时钟对表。
// 做法：先隐藏窗口，再 reload，看窗口是否会自行变可见、以及 show 是否被调用。

import { writeFileSync, mkdirSync } from "node:fs";

const PORT = 9222;
const OUT = "D:/Zcode/BILIdown/tools";
mkdirSync(OUT, { recursive: true });

async function listTargets() {
  const res = await fetch(`http://127.0.0.1:${PORT}/json/list`);
  return await res.json();
}

const pages = (await listTargets()).filter((t) => t.type === "page");
const page = pages.find((t) => t.url && t.url !== "about:blank") || pages[0];
if (!page) {
  console.error("没有可用的页面目标");
  process.exit(1);
}

const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map();
let seq = 0;
ws.addEventListener("message", (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id !== undefined) {
    const cb = pending.get(msg.id);
    if (cb) {
      pending.delete(msg.id);
      cb(msg);
    }
  }
});
function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const mid = ++seq;
    pending.set(mid, (m) => (m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result)));
    ws.send(JSON.stringify({ id: mid, method, params }));
  });
}
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable");
await send("Page.enable");

const PROBE = `(() => {
  const P = { t0: Date.now(), marks: [] };
  window.__bilidownProbe = P;
  const t = () => Date.now() - P.t0;
  const mark = (name) => P.marks.push([t(), name]);

  mark('docStart');

  let sawHtml = false;
  (function waitHtml() {
    if (document.documentElement) {
      if (!sawHtml) {
        sawHtml = true;
        mark('htmlExists');
        mark('themeAttr@birth=' + document.documentElement.getAttribute('data-theme'));
        try {
          new MutationObserver(() => mark('themeAttr=' + document.documentElement.getAttribute('data-theme')))
            .observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
        } catch (e) {}
      }
      return;
    }
    setTimeout(waitHtml, 1);
  })();

  try {
    new MutationObserver(() => {
      const el = document.getElementById('app');
      if (el && el.childElementCount > 0 && !P.appFilled) {
        P.appFilled = true;
        mark('appFilled');
      }
    }).observe(document, { childList: true, subtree: true });
  } catch (e) {}

  requestAnimationFrame(() => requestAnimationFrame(() => {
    try {
      mark('bg@paint=' + getComputedStyle(document.documentElement).backgroundColor + '/' + getComputedStyle(document.body).backgroundColor);
    } catch (e) {}
    mark('painted2raf');
  }));

  // 窗口可见性：直接用原始 IPC 轮询（模块说明符在注入脚本里解析不了）。
  // 关键：与上面的 DOM 里程碑同一条时钟，可判定「先绘制还是先显示」。
  (function pollVis() {
    const I = window.__TAURI_INTERNALS__;
    if (!I || typeof I.invoke !== 'function') return setTimeout(pollVis, 1);
    let prev = null;
    let done = 0;
    setInterval(async () => {
      if (done > 400) return;
      done += 1;
      try {
        const v = await I.invoke('plugin:window|is_visible', { label: 'main' });
        if (v !== prev) { prev = v; mark('visible=' + v); }
      } catch (e) {}
    }, 10);
  })();
})();`;

await send("Page.addScriptToEvaluateOnNewDocument", { source: PROBE });

// 先把窗口藏起来（用原始 IPC，模块说明符在 evaluate 里解析不了），
// 这样 reload 之后「窗口何时变可见」就是一次干净事件。
const hidden = await send("Runtime.evaluate", {
  expression: `(async () => {
    const I = window.__TAURI_INTERNALS__;
    try {
      await I.invoke('plugin:window|hide', { label: 'main' });
      await new Promise((r) => setTimeout(r, 300));
      const v = await I.invoke('plugin:window|is_visible', { label: 'main' });
      return 'hidden, is_visible=' + v;
    } catch (e) { return 'hide-failed: ' + e; }
  })()`,
  awaitPromise: true,
  returnByValue: true,
});
console.log("隐藏窗口: " + (hidden?.result?.value ?? "?"));

await send("Page.reload", { ignoreCache: false });

await new Promise((r) => setTimeout(r, 4500));

const probe = await send("Runtime.evaluate", {
  expression: `(() => {
    const P = window.__bilidownProbe;
    if (!P) return JSON.stringify({ error: 'probe missing' });
    return JSON.stringify({ marks: P.marks, appFilled: !!P.appFilled });
  })()`,
  awaitPromise: true,
  returnByValue: true,
});

writeFileSync(`${OUT}/cdp-visibility.json`, probe?.result?.value || "{}");

let parsed;
try {
  parsed = JSON.parse(probe?.result?.value || "{}");
} catch {
  parsed = { error: "解析失败", raw: probe?.result?.value };
}

console.log("=== 页面内时间线（同一条时钟）===");
if (parsed.marks) {
  for (const [t, name] of parsed.marks) {
    console.log(`t=${String(t).padStart(5)}ms  ${name}`);
  }
} else {
  console.log(JSON.stringify(parsed, null, 2));
}

ws.close();
process.exit(0);
