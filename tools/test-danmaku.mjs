// 下载弹幕实测：勾上「下载弹幕」后真下一条，验证
//   ① 视频旁边多了同名的 .xml（不合成进视频）② xml 是明文的 B 站弹幕格式，条数 > 0
//   ③ 成品里没有多出字幕/弹幕轨（用 ffprobe 数流）④ 日志里能看到"弹幕已保存（N 条）"
//
// 只在**沙箱实例**里跑（会点「保存」写盘、会真下载）：
//   mkdir -p D:\Zcode\_data\bilidown-sandbox
//   cp D:\Zcode\_data\bilidown\cookies.json D:\Zcode\_data\bilidown-sandbox\
//   BILIDOWN_COOKIE_FILE='D:\Zcode\_data\bilidown-sandbox\cookies.json' \
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 ./target/release/bilidown.exe
import { readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { execFileSync } from "node:child_process";

const SANDBOX = "D:/Zcode/_data/bilidown-sandbox";
// 上一条测试可能留下同一个视频：重名处理是"跳过"，跳过就不会写旁挂文件，
// 于是这条测试会看不到封面/弹幕。开跑前先清掉沙箱里的产物。
const VIDEO_URL = process.argv[2] || "https://www.bilibili.com/video/BV17x411w7KC";

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

// 上一条沙箱测试可能还在跑：等队列空闲再清产物，别把正在写的目录删掉
for (let i = 0; i < 60; i += 1) {
  const busy = await js(`(() => {
    const rows = [...document.querySelectorAll('li.row')].map(r => r.textContent.replace(/\s+/g, ' ').trim());
    return rows.some(t => /下载中|合成中|排队|获取/.test(t));
  })()`);
  if (!busy) break;
  await wait(1500);
}
rmSync(`${SANDBOX}/downloads`, { recursive: true, force: true });

// ── 1. 媒体页：勾上「下载弹幕」+ 480P，保存 ──
await js(`document.querySelectorAll(".sidebar nav button")[3].click()`);
await wait(900);
await js(`[...document.querySelectorAll('.cats button')].find(b => b.querySelector('.label')?.textContent.trim() === '媒体')?.click()`);
await wait(600);
// 先把其它开关关掉、弹幕打开（上一条测试可能改过沙箱设置）
for (const [label, want] of [["下载封面", false], ["下载字幕", false], ["下载弹幕", true]]) {
  const state = await js(`(() => {
    const el = [...document.querySelectorAll('.card-check')].find(l => l.textContent.includes(${JSON.stringify(label)}));
    return el ? el.querySelector('input').checked : null;
  })()`);
  if (state !== null && state !== want) {
    await js(`[...document.querySelectorAll('.card-check')].find(l => l.textContent.includes(${JSON.stringify(label)})).querySelector('input').click()`);
    await wait(300);
  }
}
const box = JSON.parse(await js(`(() => {
  const el = [...document.querySelectorAll('.card-check')].find(l => l.textContent.includes('下载弹幕'));
  return JSON.stringify({ found: !!el, text: el ? el.textContent.trim() : '', checked: el ? el.querySelector('input').checked : null });
})()`));
check("媒体页有「下载弹幕」勾选框", box.found, box.text);
if (box.found && !box.checked) {
  await js(`[...document.querySelectorAll('.card-check')].find(l => l.textContent.includes('下载弹幕')).querySelector('input').click()`);
  await wait(400);
}
check("勾选框已选中", (await js(`[...document.querySelectorAll('.card-check')].find(l => l.textContent.includes('下载弹幕')).querySelector('input').checked`)) === true);
check("勾上有说明（.xml、不合成、播放器直接读）",
  (await js(`[...document.querySelectorAll('.note')].some(n => n.textContent.includes('不与视频合成'))`)) === true);
await js(`(() => {
  const field = [...document.querySelectorAll('.field')].find(f => f.querySelector(':scope > label')?.textContent.trim() === '视频清晰度');
  const sel = field.querySelector('select');
  const opt = [...sel.options].find(o => o.textContent.includes('480P'));
  sel.value = opt.value; sel.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
})()`);
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.trim() === '保存').click()`);
await wait(1400);
const saved = JSON.parse(await js(`(async () => {
  const env = await window.__TAURI_INTERNALS__.invoke('app_settings');
  return JSON.stringify({ danmaku: env.settings.download_danmaku, qn: env.settings.default_quality });
})()`));
check("设置里存下了 download_danmaku", saved.danmaku === true, JSON.stringify(saved));

// ── 2. 单个链接解析 + 下载 ──
await js(`[...document.querySelectorAll('.steps li')].find(li => li.textContent.includes('解析来源'))?.click()`);
await wait(500);
await js(`[...document.querySelectorAll('.tabs button')].find(b => b.textContent.includes('单个链接'))?.click()`);
await wait(400);
await js(`(() => {
  const el = document.querySelector('textarea') || document.querySelector('input[placeholder*="链接"]');
  Object.getOwnPropertyDescriptor(el.constructor.prototype, 'value').set.call(el, ${JSON.stringify(VIDEO_URL)});
  el.dispatchEvent(new Event('input', { bubbles: true }));
  return true;
})()`);
await wait(300);
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('开始解析')).click()`);
for (let i = 0; i < 20; i += 1) {
  await wait(1200);
  if (await js(`!!document.querySelector('.select-bar')`)) break;
}
await wait(800);
console.log("列表: " + (await js(`(() => { const r = document.querySelector('.batch-table tbody tr'); return r ? [...r.querySelectorAll('td')].map(td => td.textContent.trim()).slice(1, 4).join(' | ') : '（没解析出列表）'; })()`)));
await js(`[...document.querySelectorAll('.batch-table tbody tr .col-check input')].slice(0, 1).forEach(c => c.click())`);
await wait(400);
// 先记下已有任务条数：老任务的"已完成"不算，只等这一条新任务
const baseline = await js(`document.querySelectorAll('li.row').length`);
await js(`[...document.querySelectorAll('.select-bar button')].find(b => b.textContent.includes('下载所选'))?.click()`);

// 等任务跑完（只看新出现的那条）
let done = false;
for (let i = 0; i < 90; i += 1) {
  await wait(2000);
  const st = JSON.parse(await js(`(() => {
    const rows = [...document.querySelectorAll('li.row')];
    const fresh = rows.slice(0, Math.max(rows.length - ${baseline}, 0)).map(r => r.textContent.replace(/\\s+/g, ' ').trim());
    return JSON.stringify({ n: fresh.length, done: fresh.some(t => t.includes('已完成')), failed: fresh.some(t => t.includes('失败')) });
  })()`));
  if (st.n > 0 && (st.done || st.failed)) { done = st.done; if (st.failed) console.log("  新任务失败了，见沙箱日志"); break; }
}
check("任务跑完", done);
await wait(1500);

// ── 3. 磁盘上核对 ──
const walk = (dir, out = []) => {
  for (const name of readdirSync(dir)) {
    const full = `${dir}/${name}`;
    const st = statSync(full);
    if (st.isDirectory()) { if (name !== ".bilitmp") walk(full, out); }
    else out.push({ full, size: st.size });
  }
  return out;
};
// 旁挂文件是视频落盘之后才写的，可能晚半拍：轮询等它出现（最多 20 秒）
const isVideo = (p) => /\.(mp4|ts|mkv)$/i.test(p); // 封装格式由设置决定，别把扩展名写死
let files = walk(`${SANDBOX}/downloads`);
for (let i = 0; i < 20; i += 1) {
  if (files.some((f) => isVideo(f.full)) && files.some((f) => f.full.endsWith(".xml"))) break;
  await wait(1000);
  files = walk(`${SANDBOX}/downloads`);
}
console.log("沙箱产物:\n" + files.map((f) => `  ${f.full.replace(SANDBOX, "")}  ${(f.size / 1024).toFixed(0)} KB`).join("\n"));
const mp4 = files.find((f) => isVideo(f.full));
const xml = files.find((f) => f.full.endsWith(".xml"));
check("视频下下来了", !!mp4, mp4?.full);
check("旁边有同名 .xml", !!xml && !!mp4 && xml.full.replace(/\.[^./]+$/, "") === mp4.full.replace(/\.[^./]+$/, ""), xml?.full?.replace(SANDBOX, ""));

if (xml) {
  const text = readFileSync(xml.full, "utf8");
  const count = (text.match(/<d p=/g) || []).length;
  console.log(`弹幕文件：${(text.length / 1024).toFixed(0)} KB，${count} 条，开头 ${JSON.stringify(text.slice(0, 56))}`);
  check("是明文的 B 站弹幕 XML", text.trimStart().startsWith("<?xml") && text.includes("<i>"));
  check("弹幕条数 > 0", count > 0, `${count} 条`);
  check("条目格式是播放器认的 <d p=…>", /<d p="[\d.,]+">/.test(text));
  const secs = [...text.matchAll(/<d p="([\d.]+),/g)].map((m) => parseFloat(m[1]));
  const span = Math.max(...secs);
  console.log(`时间跨度：${secs[0].toFixed(1)}s → ${span.toFixed(1)}s`);
  check("弹幕按时间排序（第一条是最早的）", secs[0] <= secs[1]);
  // 视频超过 6 分钟时，弹幕要跨过第一段（360s）才算把分段都取全了
  if (process.argv[3] === "long") {
    check("超过 6 分钟的视频：弹幕跨过了第一段（说明多段都取了）", span > 360, `最晚 ${span.toFixed(1)}s`);
  }
}

if (mp4) {
  // 成品里不该多出字幕/弹幕轨：流类型应当只有 video / audio
  let kinds = "";
  try {
    kinds = execFileSync("ffprobe", ["-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0", mp4.full], { encoding: "utf8" }).trim();
  } catch (e) { kinds = `ffprobe 失败: ${e.message}`; }
  console.log("成品流类型: " + JSON.stringify(kinds));
  // TS 容器下 ffprobe 会把同一组流打印两遍，比集合而不是比字符串
  const kindsSet = [...new Set(kinds.split(/\r?\n/).map((s) => s.trim()).filter(Boolean))].sort();
  check("成品里没有多出字幕/弹幕轨", JSON.stringify(kindsSet) === JSON.stringify(["audio", "video"]), kindsSet.join(" + "));
}

// ── 4. 日志里的证据 ──
try {
  const logs = readdirSync(`${SANDBOX}/logs`).filter((n) => n.endsWith(".log"));
  const text = logs.map((n) => readFileSync(`${SANDBOX}/logs/${n}`, "utf8")).join("\n");
  const hit = text.split(/\r?\n/).filter((l) => l.includes("弹幕")).slice(-3);
  console.log("日志里的弹幕记录: " + (hit.length ? hit.join(" ｜ ") : "（没有）"));
  check("日志里能看到弹幕已保存（含条数）", hit.some((l) => /弹幕已保存（\d+ 条）/.test(l)), hit.join(" | "));
} catch (e) {
  check("日志里能看到弹幕已保存（含条数）", false, `读日志失败: ${e.message}`);
}

console.log("\n控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
ws.close(); process.exit(fails ? 1 : 0);
