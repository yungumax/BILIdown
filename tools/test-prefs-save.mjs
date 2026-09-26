// 媒体页优先顺序：改完保存 → 从后端读回，确认真的落盘生效（不是只改了界面）
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter(t => t.type === "page").find(t => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0; const errors = [];
ws.addEventListener("message", ev => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Runtime.exceptionThrown") errors.push((m.params?.exceptionDetails?.exception?.description || "").slice(0, 200));
});
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, m => m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result)); ws.send(JSON.stringify({ id, method, params })); });
await new Promise(r => ws.addEventListener("open", r));
await send("Runtime.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;

const rows = () => js(`JSON.stringify([...document.querySelectorAll('.sub-card')][0].querySelectorAll('.pref-row').length ? [...[...document.querySelectorAll('.sub-card')][0].querySelectorAll('.pref-row')].map(r => r.querySelectorAll('select')[0].selectedOptions[0].textContent.trim() + ' / ' + r.querySelectorAll('select')[1].selectedOptions[0].textContent.trim()) : [])`);
const audioRows = () => js(`JSON.stringify([...[...document.querySelectorAll('.sub-card')][1].querySelectorAll('.pref-row')].map(r => r.querySelector('select').selectedOptions[0].textContent.trim()))`);

console.log("A) 保存前界面: " + await rows());
// 点保存
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.trim() === '保存').click()`);
await new Promise(r => setTimeout(r, 900));
const backend = await js(`(async () => {
  const s = await window.__TAURI_INTERNALS__.invoke('app_settings');
  return JSON.stringify({ quality: s.settings.quality_prefs, audio: s.settings.audio_prefs });
})()`);
console.log("B) 保存后后端: " + backend);
console.log("C) 音频界面: " + await audioRows());
console.log("D) 控制台错误: " + (errors.length ? errors.join(" | ") : "无"));

// 还原成默认，别把测试值留在你的配置里
await js(`(async () => {
  const I = window.__TAURI_INTERNALS__;
  const cur = await I.invoke('app_settings');
  const next = { ...cur.settings, quality_prefs: [{ qn: 127, codec: 'auto' }], audio_prefs: ['auto'] };
  await I.invoke('update_settings', { settings: next });
  return 'done';
})()`);
const after = await js(`(async () => JSON.stringify((await window.__TAURI_INTERNALS__.invoke('app_settings')).settings.quality_prefs))()`);
console.log("E) 已还原为: " + after);
ws.close(); process.exit(0);
