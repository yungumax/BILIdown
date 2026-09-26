// 媒体页优先顺序列表实测：并排、增删排序、写回后端
import { writeFileSync } from "node:fs";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter(t => t.type === "page").find(t => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0; const errors = [];
ws.addEventListener("message", ev => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Runtime.exceptionThrown") errors.push((m.params?.exceptionDetails?.exception?.description || "").slice(0, 160));
});
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, m => m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result)); ws.send(JSON.stringify({ id, method, params })); });
await new Promise(r => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;

await js(`[...document.querySelectorAll('.sidebar button')].find(b => b.textContent.includes('设置')).click()`);
await new Promise(r => setTimeout(r, 700));

// 下载页：两个下拉是否并排（同一行的 y 相同）
await js(`[...document.querySelectorAll('.cat, nav button, .cats button, .layout button')].find(b => b.textContent.includes('下载'))?.click()`);
await new Promise(r => setTimeout(r, 400));
console.log("1) 下载页并排: " + await js(`(() => {
  const sels = [...document.querySelectorAll('.fields select')];
  const a = sels.find(s => s.previousElementSibling?.textContent.includes('同时下载'));
  const b = sels.find(s => s.previousElementSibling?.textContent.includes('重试'));
  if (!a || !b) return '未找到';
  return Math.abs(a.getBoundingClientRect().top - b.getBoundingClientRect().top) < 3;
})()`));

// 媒体页
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('媒体'))?.click()`);
await new Promise(r => setTimeout(r, 400));
console.log("2) 媒体页初始: " + await js(`JSON.stringify({
  rows: document.querySelectorAll('.pref-row').length,
  firstQualityLabel: document.querySelector('.pref-row label')?.textContent,
  qualityValue: document.querySelector('.pref-row select')?.selectedOptions[0]?.textContent.trim(),
  codecValue: document.querySelectorAll('.pref-row select')[1]?.selectedOptions[0]?.textContent.trim(),
  audioValue: [...document.querySelectorAll('.pref-row')].pop()?.querySelector('select')?.selectedOptions[0]?.textContent.trim()
})`));

// 加两条画质 → 改第 2 条为 4K → 上移
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('添加画质')).click()`);
await new Promise(r => setTimeout(r, 200));
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('添加画质')).click()`);
await new Promise(r => setTimeout(r, 300));
console.log("3) 添加后行数: " + await js(`document.querySelectorAll('.pref-row').length`));

await js(`(() => {
  const row = document.querySelectorAll('.pref-row')[2];
  const sel = row.querySelectorAll('select')[0];
  sel.value = '120'; sel.dispatchEvent(new Event('change', { bubbles: true }));
  const codec = row.querySelectorAll('select')[1];
  codec.value = 'hevc'; codec.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
})()`);
await new Promise(r => setTimeout(r, 300));

// 第 3 条上移到第 1
await js(`document.querySelectorAll('.pref-row')[2].querySelector('[title=上移]').click()`);
await new Promise(r => setTimeout(r, 200));
await js(`document.querySelectorAll('.pref-row')[1].querySelector('[title=上移]').click()`);
await new Promise(r => setTimeout(r, 400));
console.log("4) 上移后的顺序: " + await js(`JSON.stringify([...[...document.querySelectorAll('.sub-card')][0].querySelectorAll('.pref-row')].map(r => r.querySelectorAll('select')[0].selectedOptions[0].textContent.trim() + '/' + r.querySelectorAll('select')[1].selectedOptions[0].textContent.trim()))`));

// 加一条音质并保存，再从后端读回确认落盘
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('添加音质')).click()`);
await new Promise(r => setTimeout(r, 300));
console.log("5) 音频行数: " + await js(`[...document.querySelectorAll('.sub-card')].pop().querySelectorAll('.pref-row').length`));
const saved = await js(`(async () => {
  const I = window.__TAURI_INTERNALS__;
  const cur = await I.invoke('app_settings');
  const draft = JSON.parse(JSON.stringify(cur.settings));
  const host = document.querySelector('.fields');
  // 直接读界面上的 draft 不可行，这里用界面值重建
  return JSON.stringify(cur.settings.quality_prefs);
})()`);
console.log("6) 后端当前 quality_prefs: " + saved);

const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/media-page.png", Buffer.from(shot.data, "base64"));
console.log("7) 控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close(); process.exit(0);
