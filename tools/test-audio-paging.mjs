// 音频「解析全部」翻到底：不该再报"格式响应异常"，并且要如实说明可下载条数
const URL_AUDIO = process.argv[2] || "https://space.bilibili.com/649910/upload/audio";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0; const errors = [];
ws.addEventListener("message", (ev) => { const m = JSON.parse(ev.data); if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; } if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text.slice(0, 200)); });
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await wait(400);
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await wait(300);
await js(`(() => { const el = document.querySelector("textarea"); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, ${JSON.stringify(URL_AUDIO)}); el.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await wait(300);
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 14; i += 1) { await wait(1400); if (await js(`!!document.querySelector(".select-bar")`)) break; }
await wait(800);
console.log("解析后: " + await js(`JSON.stringify({ 计数: document.querySelector(".select-bar .loaded-hint")?.textContent.trim() })`));

const toasts = [];
js(`window.__toasts = []; (() => { const el = document.querySelector(".toast"); if (el) window.__toasts.push(el.textContent.trim()); })()`);
await js(`document.querySelector(".parse-split button.arrow")?.click()`);
await wait(400);
await js(`[...document.querySelectorAll(".parse-item")].find(b => b.textContent.includes("解析全部"))?.click()`);
for (let i = 0; i < 60; i += 1) {
  await wait(1300);
  if (await js(`!document.querySelector(".parse-split")`)) break;
  const t = await js(`document.querySelector(".toast")?.textContent.trim() || ""`);
  if (t && !toasts.includes(t)) toasts.push(t);
}
await wait(800);
console.log("最终: " + await js(`JSON.stringify({
  计数: document.querySelector(".select-bar .loaded-hint")?.textContent.trim(),
  提示: document.querySelector(".note")?.textContent.trim() || "无",
  行数: [...document.querySelectorAll(".batch-table tbody tr")].filter(tr => tr.querySelector(".col-check input")).length,
  按钮: [...document.querySelectorAll(".select-bar button")].map(b => b.textContent.trim()).filter(Boolean).join(" | ")
})`));
console.log("过程中出现的提示: " + (toasts.length ? toasts.join(" | ") : "无"));
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close(); process.exit(0);
