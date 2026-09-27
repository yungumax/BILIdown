import { writeFileSync } from "node:fs";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0; const errors = [];
ws.addEventListener("message", (ev) => { const m = JSON.parse(ev.data); if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; } if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text.slice(0, 160)); });
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const shot = async (n) => { const s = await send("Page.captureScreenshot", { format: "png" }); writeFileSync("D:/Zcode/BILIdown/tools/" + n, Buffer.from(s.data, "base64")); };
const pick = async (label) => {
  await js(`[...document.querySelectorAll(".titlebar button")].find((b) => b.getAttribute("aria-haspopup") === "menu")?.click()`);
  await wait(400);
  await js(`[...document.querySelectorAll(".theme-menu button")].find((b) => b.textContent.includes(${JSON.stringify(label)}))?.click()`);
  await wait(900);
};
const contrast = `(() => {
  const rgb = (css) => (css.match(/\d+(\.\d+)?/g) || []).slice(0, 3).map(Number);
  const lum = ([r, g, b]) => { const f = (c) => { c /= 255; return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return ((x + 0.05) / (y + 0.05)).toFixed(2); };
  const cs = getComputedStyle(document.documentElement);
  const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const card = rgb(getComputedStyle(document.querySelector(".card") || document.body).backgroundColor);
  return JSON.stringify({
    主题: document.documentElement.dataset.theme,
    正文与卡片: ratio(rgb(cs.getPropertyValue("color")), card),
    次要文字与卡片: ratio(hex(cs.getPropertyValue("--muted").trim()), card),
    弱化文字与卡片: ratio(hex(cs.getPropertyValue("--faint").trim()), card),
    选区底色: cs.getPropertyValue("--accent-soft").trim(),
    圆角: cs.getPropertyValue("--radius-lg").trim()
  });
})()`;
await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await wait(700);
console.log("深色: " + (await js(contrast)));
await shot("v2opt-parse-dark.png");
await js(`[...document.querySelectorAll(".sidebar button, .sidebar li")].find(b => b.textContent.includes("设置"))?.click()`);
await wait(800);
await shot("v2opt-settings-dark.png");
await pick("浅色");
await shot("v2opt-settings-light.png");
await js(`[...document.querySelectorAll(".sidebar button, .sidebar li")].find(b => b.textContent.includes("解析"))?.click()`);
await wait(800);
console.log("浅色: " + (await js(contrast)));
await shot("v2opt-parse-light.png");
await pick("深色");
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close(); process.exit(0);
