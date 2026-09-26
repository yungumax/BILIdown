// 真实点击主题按钮，录下主题变化与 toast 文本。
// 用法：node tools/cdp-toggle.mjs  （需要应用以 --remote-debugging-port=9222 启动）

import { writeFileSync, mkdirSync } from "node:fs";

const PORT = 9222;
const OUT = "D:/Zcode/BILIdown/tools";
const SHOTS = `${OUT}/shots-toggle`;
mkdirSync(SHOTS, { recursive: true });

const T0 = Date.now();

async function listTargets() {
  const res = await fetch(`http://127.0.0.1:${PORT}/json/list`);
  return await res.json();
}

const pages = (await listTargets()).filter((t) => t.type === "page");
const page = pages.find((t) => t.url && t.url !== "about:blank") || pages[0];
if (!page) {
  console.error("没有可用的页面目标：请确认应用已启动且带调试端口");
  process.exit(1);
}

const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map();
const events = [];
let seq = 0;

ws.addEventListener("message", (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id !== undefined) {
    const cb = pending.get(msg.id);
    if (cb) {
      pending.delete(msg.id);
      cb(msg);
    }
    return;
  }
  const p = msg.params || {};
  if (msg.method === "Runtime.consoleAPICalled") {
    events.push({ t: Date.now() - T0, kind: `console.${p.type}`, text: (p.args || []).map((a) => a.value ?? a.description ?? a.type).join(" | ") });
  } else if (msg.method === "Runtime.exceptionThrown") {
    const d = p.exceptionDetails || {};
    events.push({ t: Date.now() - T0, kind: "exception", text: `${d.text || ""} :: ${d.exception?.description || ""}`.slice(0, 300) });
  } else if (msg.method === "Log.entryAdded") {
    events.push({ t: Date.now() - T0, kind: `log.${p.entry?.level}`, text: (p.entry?.text || "").slice(0, 300) });
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
await send("Log.enable");
await send("Page.enable");

const READ = `JSON.stringify({
  theme: document.documentElement.dataset.theme || null,
  htmlBg: getComputedStyle(document.documentElement).backgroundColor,
  bodyBg: document.body ? getComputedStyle(document.body).backgroundColor : '',
  toast: document.querySelector('.toast') ? document.querySelector('.toast').textContent : '',
  btnTitle: document.querySelector('.titlebar .icon-btn')?.title || '',
})`;

async function read() {
  const r = await send("Runtime.evaluate", { expression: READ, returnByValue: true });
  return r?.result?.value || "";
}

const samples = [];
let last = await read();
samples.push({ t: Date.now() - T0, state: last, note: "初始" });

// 真实点击主题按钮（和用户点击同一个元素）
const click = await send("Runtime.evaluate", {
  expression: `(() => {
    const b = document.querySelector('.titlebar .icon-btn');
    if (!b) return 'no-button';
    b.click();
    return 'clicked';
  })()`,
  returnByValue: true,
});
const clickResult = click?.result?.value;
events.push({ t: Date.now() - T0, kind: "action", text: `点击主题按钮: ${clickResult}` });

let shotIndex = 0;
let nextShot = 0;
const shots = [];
const DURATION = 3500;

while (Date.now() - T0 < DURATION) {
  const now = Date.now() - T0;
  const cur = await read();
  if (cur !== last) {
    samples.push({ t: now, state: cur });
    last = cur;
  }
  if (now >= nextShot) {
    nextShot = now + 150;
    try {
      const shot = await send("Page.captureScreenshot", { format: "png" });
      const file = `${SHOTS}/t${String(shotIndex).padStart(2, "0")}_${String(now).padStart(4, "0")}.png`;
      writeFileSync(file, Buffer.from(shot.data, "base64"));
      shots.push({ t: now, file: file.split("/").pop() });
      shotIndex += 1;
    } catch {
      // 忽略
    }
  }
  await new Promise((r) => setTimeout(r, 40));
}

writeFileSync(`${OUT}/cdp-toggle.json`, JSON.stringify({ samples, events, shots }, null, 2));

console.log("=== 点击主题按钮之后 ===");
for (const s of samples) {
  let o;
  try {
    o = JSON.parse(s.state);
  } catch {
    o = { raw: s.state };
  }
  console.log(
    `t=${String(s.t).padStart(5)}ms theme=${o.theme} htmlBg=${o.htmlBg}${o.toast ? `  TOAST="${o.toast}"` : ""} btn="${o.btnTitle}"`
  );
}
console.log("=== 控制台 / 异常 ===");
if (events.length === 0) console.log("(无)");
for (const e of events) console.log(`t=${String(e.t).padStart(5)}ms [${e.kind}] ${e.text}`);

ws.close();
process.exit(0);
