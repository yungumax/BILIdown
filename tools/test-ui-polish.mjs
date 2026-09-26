// 核对四项改动：单个视频不显示来源行、勾选框外观、提示图标为圆圈问号、提示文本
import { writeFileSync } from "node:fs";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter(t => t.type === "page").find(t => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0;
ws.addEventListener("message", ev => { const m = JSON.parse(ev.data); if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } } });
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, m => m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result)); ws.send(JSON.stringify({ id, method, params })); });
await new Promise(r => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;

// 1) 解析页：批量 / 单个视频 两种模式下 sources 行是否出现
await js(`[...document.querySelectorAll('.sidebar button')].find(b => b.textContent.includes('解析')).click()`);
await new Promise(r => setTimeout(r, 500));
console.log("1) 批量模式 sources 行: " + await js(`!!document.querySelector('.sources')`));
await js(`[...document.querySelectorAll('.tabs button')].find(b => b.textContent.includes('单个视频')).click()`);
await new Promise(r => setTimeout(r, 400));
console.log("   单个视频模式 sources 行: " + await js(`!!document.querySelector('.sources')`));
console.log("   单个视频模式输入框仍在: " + await js(`!!document.querySelector('textarea, input[spellcheck]')`));
await js(`[...document.querySelectorAll('.tabs button')].find(b => b.textContent.includes('批量解析')).click()`);
await new Promise(r => setTimeout(r, 300));
console.log("   切回批量 sources 行: " + await js(`!!document.querySelector('.sources')`));

// 2) 设置页：勾选框与提示图标
await js(`[...document.querySelectorAll('.sidebar button')].find(b => b.textContent.includes('设置')).click()`);
await new Promise(r => setTimeout(r, 600));
await js(`[...document.querySelectorAll('.cat, .layout button')].find(b => b.textContent.includes('下载'))?.click()`);
await new Promise(r => setTimeout(r, 500));
console.log("2) 勾选框: " + await js(`JSON.stringify([...document.querySelectorAll('input[type=checkbox]')].slice(0,2).map(e => {
  const cs = getComputedStyle(e);
  return { appearance: cs.appearance, radius: cs.borderRadius, checked: e.checked, bg: cs.backgroundColor };
}))`));
console.log("3) 提示图标: " + await js(`JSON.stringify([...document.querySelectorAll('.info')].map(e => ({ text: e.textContent, title: e.title, round: getComputedStyle(e).borderRadius, size: Math.round(e.getBoundingClientRect().width) + 'x' + Math.round(e.getBoundingClientRect().height) })))`));
const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/polish-check.png", Buffer.from(shot.data, "base64"));
ws.close(); process.exit(0);
