// 实测主题菜单：展开 → 选一项 → 生效并收起；顺便检查没有 toast
import { writeFileSync } from "node:fs";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter(t => t.type === "page").find(t => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0;
const events = [];
ws.addEventListener("message", ev => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Runtime.exceptionThrown") events.push("异常: " + (m.params?.exceptionDetails?.exception?.description || "").slice(0, 200));
  if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") events.push("console.error: " + (m.params.args || []).map(a => a.value ?? a.description).join(" "));
});
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, m => m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result)); ws.send(JSON.stringify({ id, method, params })); });
await new Promise(r => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Page.enable");

const read = async () => JSON.parse((await send("Runtime.evaluate", { expression: `JSON.stringify({
  theme: document.documentElement.dataset.theme,
  bg: getComputedStyle(document.documentElement).backgroundColor,
  menuOpen: !!document.querySelector('.theme-menu'),
  items: [...document.querySelectorAll('.theme-item')].map(e => e.textContent.trim() + (e.classList.contains('active') ? '(选中)' : '')),
  btnTitle: document.querySelector('.theme-picker .icon-btn')?.title || '',
  toast: document.querySelector('.toast')?.textContent || ''
})`, returnByValue: true })).result.value);

console.log("初始:            " + JSON.stringify(await read()));

// 1) 点按钮展开
await send("Runtime.evaluate", { expression: `document.querySelector('.theme-picker .icon-btn').click()`, returnByValue: true });
await new Promise(r => setTimeout(r, 400));
console.log("点击按钮后:      " + JSON.stringify(await read()));
let shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/theme-menu-open.png", Buffer.from(shot.data, "base64"));

// 2) 点「深色」
await send("Runtime.evaluate", { expression: `[...document.querySelectorAll('.theme-item')].find(e => e.textContent.includes('深色')).click()`, returnByValue: true });
await new Promise(r => setTimeout(r, 700));
console.log("选「深色」后:    " + JSON.stringify(await read()));
shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/theme-menu-dark.png", Buffer.from(shot.data, "base64"));

// 3) 再展开 → 点外部关闭
await send("Runtime.evaluate", { expression: `document.querySelector('.theme-picker .icon-btn').click()`, returnByValue: true });
await new Promise(r => setTimeout(r, 300));
console.log("再次展开:        " + JSON.stringify(await read()));
await send("Runtime.evaluate", { expression: `document.querySelector('.content').dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))`, returnByValue: true });
await new Promise(r => setTimeout(r, 300));
console.log("点外部后:        " + JSON.stringify(await read()));

// 4) Esc 关闭
await send("Runtime.evaluate", { expression: `document.querySelector('.theme-picker .icon-btn').click()`, returnByValue: true });
await new Promise(r => setTimeout(r, 250));
await send("Runtime.evaluate", { expression: `document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))`, returnByValue: true });
await new Promise(r => setTimeout(r, 250));
console.log("按 Esc 后:       " + JSON.stringify(await read()));

// 5) 还原成跟随系统
await send("Runtime.evaluate", { expression: `document.querySelector('.theme-picker .icon-btn').click()`, returnByValue: true });
await new Promise(r => setTimeout(r, 250));
await send("Runtime.evaluate", { expression: `[...document.querySelectorAll('.theme-item')].find(e => e.textContent.includes('跟随系统')).click()`, returnByValue: true });
await new Promise(r => setTimeout(r, 600));
console.log("还原跟随系统后:  " + JSON.stringify(await read()));
console.log("控制台错误: " + (events.length ? events.join(" | ") : "无"));
ws.close(); process.exit(0);
