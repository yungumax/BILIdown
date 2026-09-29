// 媒体页的「视频清晰度」到底管不管用：真下载一条，看任务上的画质标签。
//
// 只在**沙箱实例**里跑（会点「保存」写盘）：
//   BILIDOWN_COOKIE_FILE='D:\Zcode\_data\bilidown-sandbox\cookies.json' \
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 ./target/release/bilidown.exe
// 沙箱里 cookie 要拷一份，不然没登录、解析不到东西。
import { writeFileSync } from "node:fs";
const URL_LIST = process.argv[2] || "https://space.bilibili.com/927587/lists/108434?type=season";

const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0;
const errors = [];
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Runtime.exceptionThrown") errors.push((m.params?.exceptionDetails?.exception?.description || "").slice(0, 160));
  if (m.method === "Log.entryAdded" && m.params?.entry?.level === "error") errors.push(m.params.entry.text.slice(0, 160));
});
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

let fails = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok  " : "  FAIL"}  ${name}${detail ? "  " + detail : ""}`);
  if (!ok) fails++;
};

// ── 1. 媒体页：视频清晰度 → 480P，保存（写的是沙箱的 settings.json） ──
await js(`document.querySelectorAll(".sidebar nav button")[3].click()`);
await wait(900);
await js(`[...document.querySelectorAll('.cats button')].find(b => b.querySelector('.label')?.textContent.trim() === '媒体')?.click()`);
await wait(600);
const picked = await js(`(() => {
  const field = [...document.querySelectorAll('.field')].find(f => f.querySelector(':scope > label')?.textContent.trim() === '视频清晰度');
  const sel = field.querySelector('select');
  const opt = [...sel.options].find(o => o.textContent.includes('480P'));
  sel.value = opt.value;
  sel.dispatchEvent(new Event('change', { bubbles: true }));
  return opt.textContent.trim();
})()`);
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.trim() === '保存').click()`);
await wait(1400);
const saved = JSON.parse(await js(`(async () => {
  const env = await window.__TAURI_INTERNALS__.invoke('app_settings');
  return JSON.stringify({ qn: env.settings.default_quality, prefs: env.settings.quality_prefs, audio: env.settings.default_audio, audioPrefs: env.settings.audio_prefs });
})()`));
console.log(`媒体页选了「${picked}」→ 后端: ${JSON.stringify(saved)}`);
check("「视频清晰度」写进了设置", saved.qn === 32, `default_quality=${saved.qn}`);
check("优先顺序表仍是空的（= 未自定义）", saved.prefs.length === 0, JSON.stringify(saved.prefs));

// ── 2. 解析一个来源 ──
await js(`[...document.querySelectorAll('.steps li')].find(li => li.textContent.includes('解析来源'))?.click()`);
await wait(500);
await js(`[...document.querySelectorAll('.tabs button')].find(b => b.textContent.includes('批量解析'))?.click()`);
await wait(300);
await js(`(() => {
  const el = document.querySelector('textarea');
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, ${JSON.stringify(URL_LIST)});
  el.dispatchEvent(new Event('input', { bubbles: true }));
  return true;
})()`);
await wait(300);
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('开始解析')).click()`);
for (let i = 0; i < 18; i += 1) {
  await wait(1400);
  if (await js(`!!document.querySelector('.select-bar')`)) break;
}
await wait(900);

// ── 3. 只下第一条 ──
await js(`[...document.querySelectorAll('.batch-table tbody tr .col-check input')].slice(0, 1).forEach(c => c.click())`);
await wait(400);
await js(`[...document.querySelectorAll('.select-bar button')].find(b => b.textContent.includes('下载所选'))?.click()`);

// ── 4. 看任务上的画质标签 ──
await js(`document.querySelectorAll(".sidebar nav button")[2]?.click()`);
let label = "";
for (let i = 0; i < 20; i += 1) {
  await wait(1200);
  label = await js(`(() => {
    const rows = [...document.querySelectorAll('li.row, .task-row, .task')];
    const hit = rows.map(r => r.textContent.replace(/\\s+/g, ' ').trim())
      .find(t => /(8K|杜比|HDR|4K|1080P|720P|480P|360P)/.test(t));
    return hit || '';
  })()`);
  if (!label && i === 3) {
    console.log("  调试 · 任务区: " + await js(`JSON.stringify({
      行数: document.querySelectorAll('li.row').length,
      原样: [...document.querySelectorAll('li.row')].slice(0, 2).map(r => r.textContent.replace(/\\s+/g, ' ').trim().slice(0, 90))
    })`));
  }
  if (label) break;
}
console.log("任务行: " + (label ? label.slice(0, 120) : "（没抓到画质标签）"));
check("按设置的 480P 挑流（而不是可用最高档）", /480P/.test(label), label.slice(0, 90));
check("没有串到更高档", !/(8K|4K|1080P|720P)/.test(label), label.slice(0, 90));

const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/media-pick.png", Buffer.from(shot.data, "base64"));
console.log("\n控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
ws.close(); process.exit(fails ? 1 : 0);
