// 真实重命名：解析来源 → 预演 → 确认 → 打印磁盘结果
const url = process.argv[2];
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0; const errs = [];
ws.addEventListener("message", (ev) => { const m = JSON.parse(ev.data); if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; } if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errs.push(m.params.entry.text.slice(0, 200)); });
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const toast = () => js(`document.querySelector(".toast")?.textContent.trim() ?? "无"`);
await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await wait(500);
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await wait(300);
await js(`(() => { const el = document.querySelector("textarea"); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, ${JSON.stringify(url)}); el.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await wait(300);
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 100; i += 1) {
  await wait(3000);
  const ready = await js(`!!document.querySelector(".select-bar")`);
  const busy = await js(`/解析中/.test(document.body.textContent)`);
  if (ready && !busy && i > 2) break;
}
await wait(2000);
console.log("来源: " + url);
console.log("计数: " + await js(`document.querySelector(".select-bar .loaded-hint")?.textContent.trim()`));
await js(`[...document.querySelectorAll(".select-bar button")].find(b => b.textContent.includes("重命名"))?.click()`);
await wait(4000);
console.log("预演: " + (await toast()));
console.log("明细首条: " + await js(`[...document.querySelectorAll(".select-bar button")].map(b => b.title).find(t => t.includes("→")) ?? "无"`));
await js(`[...document.querySelectorAll(".select-bar button")].find(b => b.textContent.includes("确认改名"))?.click()`);
await wait(6000);
console.log("执行: " + (await toast()));
console.log("控制台错误: " + (errs.length ? errs.join(" | ") : "无"));
ws.close(); process.exit(0);
