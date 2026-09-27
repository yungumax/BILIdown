// 封面 / 字幕改成独立文件后的实测：真下一条视频，验证
//   ① 旁边多了同名封面图片（真图片，不是 0 字节）② 有字幕时写出同名 .srt（时间戳格式对）
//   ③ 成品里既没有字幕轨也没有附带的封面图（= 真没合成进视频）
//   ④ 日志里能看到"封面已保存"/"字幕已保存"
//
// 只在**沙箱实例**里跑（会点保存、会真下载）：
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
const toggle = async (label, want) => {
  const state = await js(`(() => {
    const el = [...document.querySelectorAll('.card-check')].find(l => l.textContent.includes(${JSON.stringify(label)}));
    return el ? el.querySelector('input').checked : null;
  })()`);
  if (state === null) return false;
  if (state !== want) {
    await js(`[...document.querySelectorAll('.card-check')].find(l => l.textContent.includes(${JSON.stringify(label)})).querySelector('input').click()`);
    await wait(300);
  }
  return true;
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

// ── 1. 设置：勾上封面 + 字幕（弹幕关掉，这次只验这两样） ──
await js(`[...document.querySelectorAll('.sidebar button')].find(b => b.textContent.includes('设置')).click()`);
await wait(900);
await js(`[...document.querySelectorAll('.cats button')].find(b => b.querySelector('.label')?.textContent.trim() === '媒体')?.click()`);
await wait(600);
check("媒体页有「下载封面（独立图片）」", await toggle("下载封面", true));
check("媒体页有「下载字幕（独立 .srt）」", await toggle("下载字幕", true));
await toggle("下载弹幕", false);
check("说明写清了独立文件、不合成",
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
  return JSON.stringify({ cover: env.settings.download_cover, subs: env.settings.download_subtitles, danmaku: env.settings.download_danmaku });
})()`));
check("三个开关都存下了", saved.cover === true && saved.subs === true && saved.danmaku === false, JSON.stringify(saved));

// ── 2. 单个链接 → 解析 → 下载 ──
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
await js(`[...document.querySelectorAll('.batch-table tbody tr .col-check input')].slice(0, 1).forEach(c => c.click())`);
await wait(400);
// 先记下已有任务条数：老任务的"已完成"不算，只等这一条新任务
const baseline = await js(`document.querySelectorAll('li.row').length`);
await js(`[...document.querySelectorAll('.select-bar button')].find(b => b.textContent.includes('下载所选'))?.click()`);

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

// ── 3. 磁盘核对 ──
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
let files = walk(`${SANDBOX}/downloads`);
for (let i = 0; i < 20; i += 1) {
  const hasImage = files.some((f) => /\.(jpg|jpeg|png|webp)$/i.test(f.full));
  if (files.some((f) => f.full.endsWith(".mp4")) && hasImage) break;
  await wait(1000);
  files = walk(`${SANDBOX}/downloads`);
}
console.log("沙箱产物:\n" + files.map((f) => `  ${f.full.replace(SANDBOX, "")}  ${(f.size / 1024).toFixed(0)} KB`).join("\n"));
const stem = (p) => p.replace(/\.[^./]+$/, "");
const mp4 = files.find((f) => f.full.endsWith(".mp4"));
const cover = files.find((f) => /\.(jpg|jpeg|png|webp)$/i.test(f.full));
const srt = files.find((f) => f.full.endsWith(".srt"));
check("视频下下来了", !!mp4, mp4?.full?.replace(SANDBOX, ""));
check("旁边有同名封面图片", !!cover && !!mp4 && stem(cover.full) === stem(mp4.full), cover?.full?.replace(SANDBOX, ""));

if (cover) {
  const head = readFileSync(cover.full).subarray(0, 12);
  const isJpeg = head[0] === 0xff && head[1] === 0xd8;
  const isPng = head[0] === 0x89 && head.toString("latin1", 1, 4) === "PNG";
  const isWebp = head.toString("latin1", 0, 4) === "RIFF" && head.toString("latin1", 8, 12) === "WEBP";
  console.log(`封面：${cover.size} 字节，magic=${head.subarray(0, 4).toString("hex")}`);
  check("封面是张真图片（JPEG / PNG / WebP 头）", isJpeg || isPng || isWebp, head.toString("hex"));
  check("封面不是空文件", cover.size > 1024, `${cover.size} 字节`);
}

if (srt) {
  const text = readFileSync(srt.full, "utf8");
  console.log(`字幕：${text.length} 字符，前 80 字 ${JSON.stringify(text.slice(0, 80))}`);
  check("字幕是同名 .srt", stem(srt.full) === stem(mp4.full));
  check("SRT 时间戳格式对（00:00:01,234 --> 00:00:04,500）",
    /^1\r?\n\d{2}:\d{2}:\d{2},\d{3} --> \d{2}:\d{2}:\d{2},\d{3}\r?\n/.test(text), text.slice(0, 40));
  check("字幕有条目内容", (text.match(/-->/g) || []).length >= 3, `${(text.match(/-->/g) || []).length} 条`);
}

if (mp4) {
  const kinds = execFileSync("ffprobe", ["-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0", mp4.full], { encoding: "utf8" }).trim();
  console.log("成品流类型: " + JSON.stringify(kinds));
  check("成品里没有字幕轨（没合成）", !/subtitle/.test(kinds), kinds);
  const vstreams = execFileSync("ffprobe", ["-v", "error", "-show_entries", "stream=index,codec_type,disposition=attached_pic", "-of", "csv=p=0", mp4.full], { encoding: "utf8" }).trim();
  console.log("成品流明细: " + JSON.stringify(vstreams));
  check("成品里没有附带的封面图（没合成）", !/attached_pic=1/.test(vstreams) && vstreams.split(/\r?\n/).length === 2, vstreams);
}

// ── 4. 日志证据 ──
const logs = readdirSync(`${SANDBOX}/logs`).filter((n) => n.endsWith(".log"));
const text = logs.map((n) => readFileSync(`${SANDBOX}/logs/${n}`, "utf8")).join("\n");
const hits = text.split(/\r?\n/).filter((l) => /封面|字幕/.test(l)).slice(-6);
console.log("日志: " + (hits.length ? hits.join("\n       ") : "（没有相关记录）"));
check("日志里能看到封面已保存", hits.some((l) => l.includes("封面已保存")));
// 字幕有三种诚实结局：写出文件 / 明说没有可用字幕 / 被风控挡住（说明原因）。
// 唯一不合格的是"什么都没记"。
const subLines = hits.filter((l) => l.includes("字幕"));
check("字幕这条有明确交代（保存了 / 没有 / 被风控挡住）",
  subLines.some((l) => /字幕已保存|没有可用字幕|字幕清单获取失败|字幕都是空/.test(l)),
  subLines.join(" | ") || "（日志里没有字幕相关记录）");

console.log("\n控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
ws.close(); process.exit(fails ? 1 : 0);
