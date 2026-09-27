// 封面改成独立文件之后的实测：真下一条视频，验证
//   ① 旁边多了同名封面图片（真图片，不是 0 字节）
//   ② 弹幕也会写成同名 .xml（开关打开时；弹幕本身的格式检查在 tools/test-danmaku.mjs）
//   ③ 成品里既没有字幕轨也没有附带的封面图（= 真没合成进视频）
//   ④ 日志里能看到"封面已保存"
//
// 字幕：界面上的「下载字幕」勾选框已经撤掉（B 站 gaia 风控把 x/player/wbi/v2 挡成 412，
// 拿不到可信字幕；未签名的 x/player/v2 会给**别的视频**的字幕，所以整条路都不用了）。
// 这里反过来验证"撤掉之后不会偷偷生成 .srt"。
//
// 只在**沙箱实例**里跑（会点保存、会真下载）：
//   mkdir -p D:\Zcode\_data\bilidown-sandbox
//   cp D:\Zcode\_data\bilidown\cookies.json D:\Zcode\_data\bilidown-sandbox\
//   BILIDOWN_COOKIE_FILE='D:\Zcode\_data\bilidown-sandbox\cookies.json' \
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 ./target/release/bilidown.exe
import { readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { execFileSync } from "node:child_process";

const SANDBOX = "D:/Zcode/_data/bilidown-sandbox";
const VIDEO_URL = process.argv[2] || "https://www.bilibili.com/video/BV17x411w7KC";
/** 成品可能是 mp4 也可能是 ts（看设置里的封装格式），别把扩展名写死 */
const isVideo = (p) => /\.(mp4|ts|mkv)$/i.test(p);
const isImage = (p) => /\.(jpg|jpeg|png|webp)$/i.test(p);

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

// ── 1. 设置：勾上封面 + 弹幕；确认「下载字幕」已经不在界面上 ──
await js(`[...document.querySelectorAll('.sidebar button')].find(b => b.textContent.includes('设置')).click()`);
await wait(900);
await js(`[...document.querySelectorAll('.cats button')].find(b => b.querySelector('.label')?.textContent.trim() === '媒体')?.click()`);
await wait(600);
check("媒体页有「下载封面（独立图片）」", await toggle("下载封面", true));
check("媒体页有「下载弹幕（独立 .xml）」", await toggle("下载弹幕", true));
const subCheckbox = await js(`[...document.querySelectorAll('.card-check')].some(l => l.textContent.includes('下载字幕'))`);
check("界面上的「下载字幕」已经撤掉（风控拿不到可信字幕）", subCheckbox === false);
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
check("封面 / 弹幕开关存下了，字幕没被打开", saved.cover === true && saved.danmaku === true && saved.subs === false, JSON.stringify(saved));

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
  if (files.some((f) => isVideo(f.full)) && files.some((f) => isImage(f.full))) break;
  await wait(1000);
  files = walk(`${SANDBOX}/downloads`);
}
console.log("沙箱产物:\n" + files.map((f) => `  ${f.full.replace(SANDBOX, "")}  ${(f.size / 1024).toFixed(0)} KB`).join("\n"));
const stem = (p) => p.replace(/\.[^./]+$/, "");
const video = files.find((f) => isVideo(f.full));
const cover = files.find((f) => isImage(f.full));
const danmaku = files.find((f) => f.full.endsWith(".xml"));
const srt = files.find((f) => f.full.endsWith(".srt"));
check("视频下下来了", !!video, video?.full?.replace(SANDBOX, ""));
check("旁边有同名封面图片", !!cover && !!video && stem(cover.full) === stem(video.full), cover?.full?.replace(SANDBOX, ""));
check("旁边有同名弹幕 .xml", !!danmaku && !!video && stem(danmaku.full) === stem(video.full), danmaku?.full?.replace(SANDBOX, ""));
check("没有生成 .srt（字幕已从界面撤掉）", !srt, srt?.full?.replace(SANDBOX, "") || "无");

if (cover) {
  const head = readFileSync(cover.full).subarray(0, 12);
  const isJpeg = head[0] === 0xff && head[1] === 0xd8;
  const isPng = head[0] === 0x89 && head.toString("latin1", 1, 4) === "PNG";
  const isWebp = head.toString("latin1", 0, 4) === "RIFF" && head.toString("latin1", 8, 12) === "WEBP";
  console.log(`封面：${cover.size} 字节，magic=${head.subarray(0, 4).toString("hex")}`);
  check("封面是张真图片（JPEG / PNG / WebP 头）", isJpeg || isPng || isWebp, head.toString("hex"));
  check("封面不是空文件", cover.size > 1024, `${cover.size} 字节`);
}

if (video) {
  const kinds = execFileSync("ffprobe", ["-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0", video.full], { encoding: "utf8" }).trim();
  // TS 容器下 ffprobe 会把同一组流打印两遍，比集合而不是比字符串
  const streamKinds = [...new Set(kinds.split(/\r?\n/).map((s) => s.trim()).filter(Boolean))].sort();
  console.log("成品流类型: " + JSON.stringify(streamKinds));
  check("成品里没有字幕轨（没合成）", !streamKinds.includes("subtitle"), kinds);
  check("成品就是一条视频 + 一条音频", JSON.stringify(streamKinds) === JSON.stringify(["audio", "video"]), kinds);
  const vstreams = execFileSync("ffprobe", ["-v", "error", "-show_entries", "stream=index,codec_type,disposition=attached_pic", "-of", "csv=p=0", video.full], { encoding: "utf8" }).trim();
  console.log("成品流明细: " + JSON.stringify(vstreams));
  check("成品里没有附带的封面图（没合成）", !/attached_pic=1/.test(vstreams), vstreams);
}

// ── 4. 日志证据 ──
const logs = readdirSync(`${SANDBOX}/logs`).filter((n) => n.endsWith(".log"));
const text = logs.map((n) => readFileSync(`${SANDBOX}/logs/${n}`, "utf8")).join("\n");
const hits = text.split(/\r?\n/).filter((l) => /封面|字幕|弹幕/.test(l)).slice(-6);
console.log("日志: " + (hits.length ? hits.join("\n       ") : "（没有相关记录）"));
check("日志里能看到封面已保存", hits.some((l) => l.includes("封面已保存")));
check("没开字幕开关时不会偷偷写字幕", !hits.some((l) => l.includes("字幕已保存")));

console.log("\n控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
ws.close(); process.exit(fails ? 1 : 0);
