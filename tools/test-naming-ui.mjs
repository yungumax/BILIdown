// 命名页实测：魔法变量面板、点击插入光标位置、预设、预览与后端渲染一致
import { writeFileSync } from "node:fs";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter(t => t.type === "page").find(t => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0;
const errors = [];
ws.addEventListener("message", ev => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Runtime.exceptionThrown") errors.push((m.params?.exceptionDetails?.exception?.description || "").slice(0, 160));
  if (m.method === "Log.entryAdded" && m.params?.entry?.level === "error") errors.push(m.params.entry.text.slice(0, 160));
});
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, m => m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result)); ws.send(JSON.stringify({ id, method, params })); });
await new Promise(r => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable");
const evalJs = async (expr) => (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result?.value;

// 进设置页 → 文件命名
await evalJs(`[...document.querySelectorAll('.sidebar button')].find(b => b.textContent.includes('设置')).click()`);
await new Promise(r => setTimeout(r, 800));
await evalJs(`[...document.querySelectorAll('.cat, .cat-item, nav button, .cats button')].find(b => b.textContent.includes('文件命名'))?.click()`);
await new Promise(r => setTimeout(r, 500));

console.log("1) 变量面板按钮存在: " + await evalJs(`!!document.querySelector('.var-picker .ghost')`));
await evalJs(`document.querySelector('.var-picker .ghost').click()`);
await new Promise(r => setTimeout(r, 400));
const panel = await evalJs(`JSON.stringify({
  open: !!document.querySelector('.var-panel'),
  head: document.querySelector('.var-head b')?.textContent,
  count: document.querySelectorAll('.var-item').length,
  first: document.querySelector('.var-item')?.textContent.replace(/\s+/g, ' ').trim(),
  last: [...document.querySelectorAll('.var-item')].pop()?.textContent.replace(/\s+/g, ' ').trim()
})`);
console.log("2) 面板: " + panel);

// 点 {part_index} 插入 → 模板应追加到末尾
await evalJs(`[...document.querySelectorAll('.var-item')].find(b => b.textContent.includes('part_index')).click()`);
await new Promise(r => setTimeout(r, 300));
console.log("3) 点击插入后模板/预览: " + await evalJs(`JSON.stringify({ tpl: document.querySelector('input[spellcheck]')?.value })`));

// 选「分P视频」预设
await evalJs(`(() => { const s = [...document.querySelectorAll('select')].find(s => [...s.options].some(o => o.textContent.includes('分P视频'))); s.value = '分P视频'; s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
await new Promise(r => setTimeout(r, 600));
const tpl = await evalJs(`document.querySelector('input[spellcheck]')?.value`);
const preview = await evalJs(`document.querySelector('.note b')?.textContent`);
// 同一个模板直接问后端，确认预览 = 后端渲染
const backend = await evalJs(`(async () => {
  const I = window.__TAURI_INTERNALS__;
  return await I.invoke('preview_naming', { template: ${JSON.stringify(tpl)}, date: '2026-09-26', publish_date: '2026-01-02', ext: 'mp4' });
})()`);
console.log("4) 预设模板: " + JSON.stringify(tpl));
console.log("5) 界面预览: " + JSON.stringify(preview));
console.log("6) 后端渲染: " + JSON.stringify(backend) + "  一致=" + (preview === backend));

// 变量清单是否与后端一致
const vars = await evalJs(`(async () => (await window.__TAURI_INTERNALS__.invoke('naming_variables')).length)()`);
console.log("7) 后端变量数=" + vars + " 面板项数=" + await evalJs(`document.querySelectorAll('.var-item').length`));

const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/naming-page.png", Buffer.from(shot.data, "base64"));
console.log("8) 控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close(); process.exit(0);
