// 通过 CDP 观察真实运行的 BILIdown：控制台报错、toast 文本、主题切换时序、逐帧截图。
// WebView2 会先有一个 about:blank 目标再导航到应用页面，因此这里先连早期目标，
// 一旦出现真正的页面目标就切过去（两边都持续收集控制台事件）。
//
// 用法：先设 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 启动 exe。

import { writeFileSync, mkdirSync } from "node:fs";

const PORT = 9222;
const OUT = "D:/Zcode/BILIdown/tools";
const SHOTS = `${OUT}/shots`;
mkdirSync(SHOTS, { recursive: true });

const T0 = Date.now();
const events = [];
const samples = [];
let seq = 0;

const isAppUrl = (u) => typeof u === "string" && u.length > 0 && u !== "about:blank";

async function listTargets() {
  try {
    const res = await fetch(`http://127.0.0.1:${PORT}/json/list`);
    return await res.json();
  } catch {
    return [];
  }
}

function attach(page, label) {
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  const pending = new Map();
  const conn = { label, ws, page, ready: false, id: page.id };

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
      events.push({
        t: Date.now() - T0,
        from: label,
        kind: `console.${p.type}`,
        text: (p.args || [])
          .map((a) => a.value ?? a.unserializableValue ?? a.description ?? a.type)
          .join(" | "),
      });
    } else if (msg.method === "Runtime.exceptionThrown") {
      const d = p.exceptionDetails || {};
      events.push({
        t: Date.now() - T0,
        from: label,
        kind: "exception",
        text: `${d.text || ""} :: ${d.exception?.description || d.exception?.value || ""}`.slice(0, 400),
      });
    } else if (msg.method === "Log.entryAdded") {
      events.push({
        t: Date.now() - T0,
        from: label,
        kind: `log.${p.entry?.level}`,
        text: `${p.entry?.text || ""} ${p.entry?.url ? "(" + p.entry.url + ")" : ""}`.slice(0, 400),
      });
    } else if (msg.method === "Page.frameNavigated") {
      events.push({ t: Date.now() - T0, from: label, kind: "navigated", text: p.frame?.url || "" });
    } else if (msg.method === "Page.loadEventFired") {
      events.push({ t: Date.now() - T0, from: label, kind: "loadEventFired", text: "" });
    } else if (msg.method === "Page.domContentEventFired") {
      events.push({ t: Date.now() - T0, from: label, kind: "domContent", text: "" });
    }
  });

  conn.send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const mid = ++seq;
      pending.set(mid, (m) => (m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result)));
      ws.send(JSON.stringify({ id: mid, method, params }));
    });

  conn.open = new Promise((resolve, reject) => {
    ws.addEventListener("open", async () => {
      try {
        await conn.send("Runtime.enable");
        await conn.send("Log.enable");
        await conn.send("Page.enable");
        conn.ready = true;
        resolve(conn);
      } catch (e) {
        reject(e);
      }
    });
    ws.addEventListener("error", () => reject(new Error("ws error")));
  });

  return conn;
}

const SAMPLE = `(() => {
  try {
    const cs = (el) => (el ? getComputedStyle(el).backgroundColor : '');
    const toast = document.querySelector('.toast');
    return JSON.stringify({
      ready: document.readyState,
      theme: document.documentElement.dataset.theme || null,
      htmlBg: cs(document.documentElement),
      bodyBg: cs(document.body),
      appBg: cs(document.querySelector('.app')),
      hasApp: !!document.querySelector('.app'),
      hasTitlebar: !!document.querySelector('.titlebar'),
      toast: toast ? toast.textContent : '',
      navLinks: document.querySelectorAll('.sidebar nav a, .sidebar button').length,
    });
  } catch (e) {
    return JSON.stringify({ ready: 'ERR', err: String(e) });
  }
})()`;

let active = null;
const conns = [];
let shotIndex = 0;
const shots = [];
let nextShot = 0;

const DURATION = 6000;
const SAMPLE_MS = 40;
const SHOT_MS = 120;

while (Date.now() - T0 < DURATION) {
  const now = Date.now() - T0;

  // 目标发现 / 切换
  const targets = await listTargets();
  const pages = targets.filter((t) => t.type === "page");
  const appPage = pages.find((t) => isAppUrl(t.url));
  const blankPage = pages.find((t) => t.type === "page");

  const want = appPage || blankPage;
  if (want && (!active || active.id !== want.id)) {
    try {
      const c = attach(want, isAppUrl(want.url) ? "app" : "blank");
      conns.push(c);
      await c.open;
      active = c;
      events.push({ t: Date.now() - T0, from: "probe", kind: "attached", text: `${c.id} ${want.url}` });
      nextShot = 0;
    } catch (e) {
      events.push({ t: Date.now() - T0, from: "probe", kind: "attach-failed", text: String(e) });
    }
  }

  if (active && active.ready) {
    try {
      const r = await active.send("Runtime.evaluate", { expression: SAMPLE, returnByValue: true });
      const state = r?.result?.value;
      if (typeof state === "string" && state.length > 0) {
        const prev = samples[samples.length - 1];
        if (!prev || prev.state !== state) samples.push({ t: now, state });
      }
    } catch {
      // 页面切换瞬间可能失败
    }

    if (now >= nextShot) {
      nextShot = now + SHOT_MS;
      try {
        const shot = await active.send("Page.captureScreenshot", { format: "png" });
        const file = `${SHOTS}/s${String(shotIndex).padStart(2, "0")}_t${String(now).padStart(4, "0")}.png`;
        writeFileSync(file, Buffer.from(shot.data, "base64"));
        shots.push({ t: now, file: file.split("/").pop() });
        shotIndex += 1;
      } catch {
        // 忽略
      }
    }
  }

  await new Promise((r) => setTimeout(r, SAMPLE_MS));
}

writeFileSync(`${OUT}/cdp-report.json`, JSON.stringify({ samples, events, shots }, null, 2));

console.log("=== 状态变化时序（DOM 层面）===");
for (const s of samples) {
  let o;
  try {
    o = JSON.parse(s.state);
  } catch {
    o = { raw: s.state };
  }
  console.log(
    `t=${String(s.t).padStart(5)}ms ready=${o.ready} theme=${o.theme} htmlBg=${o.htmlBg} bodyBg=${o.bodyBg} app=${o.hasApp} titlebar=${o.hasTitlebar}${o.toast ? `  TOAST="${o.toast}"` : ""}`
  );
}
console.log("=== 控制台 / 异常 / 导航 ===");
if (events.length === 0) console.log("(无)");
for (const e of events) console.log(`t=${String(e.t).padStart(5)}ms [${e.from}/${e.kind}] ${e.text}`);
console.log("=== 截图 ===");
for (const s of shots) console.log(`t=${String(s.t).padStart(5)}ms ${s.file}`);
