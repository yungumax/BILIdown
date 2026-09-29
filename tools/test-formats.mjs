// 三种格式的实测：视频格式 TS、音频格式 MP3、图片格式 JPG 各下一次，看真产物。
//   ① 视频：成品是 .ts，ffprobe 里视频/音频都在
//   ② 封面：选了 JPG 之后，从 webp 转成 .jpg
//   ③ 音频来源：成品是 .mp3（ffprobe 认出 mp3），不是原来的 m4a
//   ④ 图文：图片落成 .jpg（源图是 png 的那条，能证明真转过）
//
// 只在**沙箱实例**里跑（会点保存、会真下载）：
//   mkdir -p D:\Zcode\_data\bilidown-sandbox
//   cp D:\Zcode\_data\bilidown\cookies.json D:\Zcode\_data\bilidown-sandbox\
//   BILIDOWN_COOKIE_FILE='D:\Zcode\_data\bilidown-sandbox\cookies.json' \
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 ./target/release/bilidown.exe
import { readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { execFileSync } from "node:child_process";

const SANDBOX = "D:/Zcode/_data/bilidown-sandbox";
const VIDEO = "https://www.bilibili.com/video/BV17x411w7KC";
const AUDIO = "https://space.bilibili.com/649910/upload/audio";
const OPUS = "https://space.bilibili.com/486287787/upload/opus";

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
const probe = (file) => {
  try {
    return execFileSync("ffprobe", ["-v", "error", "-show_entries", "stream=codec_type,codec_name", "-of", "csv=p=0", file], { encoding: "utf8" }).trim();
  } catch (e) { return `ffprobe 失败: ${e.message}`; }
};
const walk = (dir, out = []) => {
  for (const name of readdirSync(dir)) {
    const full = `${dir}/${name}`;
    const st = statSync(full);
    if (st.isDirectory()) { if (name !== ".bilitmp") walk(full, out); }
    else out.push({ full, size: st.size });
  }
  return out;
};
const waitFiles = async (pred, seconds = 25) => {
  let files = walk(`${SANDBOX}/downloads`);
  for (let i = 0; i < seconds; i += 1) {
    if (pred(files)) return files;
    await wait(1000);
    files = walk(`${SANDBOX}/downloads`);
  }
  return files;
};
const setFormat = async (label, text) => {
  await js(`(() => {
    const field = [...document.querySelectorAll('.field')].find(f => f.querySelector(':scope > label')?.textContent.trim() === ${JSON.stringify(label)});
    const sel = field.querySelector('select');
    const opt = [...sel.options].find(o => o.textContent.includes(${JSON.stringify(text)}));
    sel.value = opt.value;
    sel.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  })()`);
  await wait(300);
};
const toggle = async (label, want) => {
  const state = await js(`(() => {
    const el = [...document.querySelectorAll('.card-check')].find(l => l.textContent.includes(${JSON.stringify(label)}));
    return el ? el.querySelector('input').checked : null;
  })()`);
  if (state === null || state === want) return;
  await js(`[...document.querySelectorAll('.card-check')].find(l => l.textContent.includes(${JSON.stringify(label)})).querySelector('input').click()`);
  await wait(300);
};
const goSettings = async () => {
  await js(`document.querySelectorAll(".sidebar nav button")[3].click()`);
  await wait(900);
  await js(`[...document.querySelectorAll('.cats button')].find(b => b.querySelector('.label')?.textContent.trim() === '媒体')?.click()`);
  await wait(600);
};
const parseAndFirst = async (url, tab = "批量解析") => {
  await js(`[...document.querySelectorAll('.steps li')].find(li => li.textContent.includes('解析来源'))?.click()`);
  await wait(500);
  await js(`[...document.querySelectorAll('.tabs button')].find(b => b.textContent.includes(${JSON.stringify(tab)}))?.click()`);
  await wait(400);
  await js(`(() => {
    const el = document.querySelector('textarea') || document.querySelector('input[placeholder*="链接"]');
    Object.getOwnPropertyDescriptor(el.constructor.prototype, 'value').set.call(el, ${JSON.stringify(url)});
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
};
const downloadFirst = async () => {
  const baseline = await js(`document.querySelectorAll('li.row').length`);
  await js(`[...document.querySelectorAll('.batch-table tbody tr .col-check input')].slice(0, 1).forEach(c => c.click())`);
  await wait(400);
  await js(`[...document.querySelectorAll('.select-bar button')].find(b => b.textContent.includes('下载所选'))?.click()`);
  for (let i = 0; i < 90; i += 1) {
    await wait(2000);
    const st = JSON.parse(await js(`(() => {
      const rows = [...document.querySelectorAll('li.row')];
      const fresh = rows.slice(0, Math.max(rows.length - ${baseline}, 0)).map(r => r.textContent.replace(/\\s+/g, ' ').trim());
      return JSON.stringify({ n: fresh.length, done: fresh.some(t => t.includes('已完成')), failed: fresh.some(t => t.includes('失败')) });
    })()`));
    if (st.n > 0 && (st.done || st.failed)) return st.done;
  }
  return false;
};

// 上一条测试可能还在跑：等队列空闲再清产物
for (let i = 0; i < 60; i += 1) {
  const busy = await js(`(() => {
    const rows = [...document.querySelectorAll('li.row')].map(r => r.textContent.replace(/\\s+/g, ' ').trim());
    return rows.some(t => /下载中|合成中|排队|获取/.test(t));
  })()`);
  if (!busy) break;
  await wait(1500);
}
rmSync(`${SANDBOX}/downloads`, { recursive: true, force: true });

// ── 1. 媒体页：视频 TS / 音频 MP3 / 图片 JPG，并打开封面下载 ──
await goSettings();
console.log("== 格式设置 ==");
await setFormat("视频格式", "TS");
await setFormat("音频格式", "MP3");
await setFormat("图片格式", "JPG");
await toggle("下载封面", true);
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.trim() === '保存').click()`);
await wait(1400);
const saved = JSON.parse(await js(`(async () => {
  const env = await window.__TAURI_INTERNALS__.invoke('app_settings');
  return JSON.stringify({ container: env.settings.container, audio: env.settings.audio_format, image: env.settings.image_format, cover: env.settings.download_cover });
})()`));
console.log("  后端读到: " + JSON.stringify(saved));
check("三个格式都存下去了", saved.container === "ts" && saved.audio === "mp3" && saved.image === "jpg", JSON.stringify(saved));
check("「下载字幕」勾选框已摘掉",
  (await js(`[...document.querySelectorAll('.card-check')].some(l => l.textContent.includes('下载字幕'))`)) === false);
check("封装的 MKV 选项没了",
  (await js(`(() => {
    const f = [...document.querySelectorAll('.field')].find(f => f.querySelector(':scope > label')?.textContent.trim() === '视频格式');
    return [...f.querySelector('select').options].some(o => o.textContent.includes('MKV'));
  })()`)) === false);
const row = JSON.parse(await js(`JSON.stringify([...document.querySelectorAll('.grid3 .field')].map(f => [Math.round(f.getBoundingClientRect().top), Math.round(f.getBoundingClientRect().left), f.querySelector(':scope > label').textContent.trim()]))`));
check("三个下拉排在同一排", row.length === 3 && row.every((r) => r[0] === row[0][0]) && row[0][1] < row[1][1] && row[1][1] < row[2][1], JSON.stringify(row));

// ── 2. 视频：成品 .ts + 封面 .jpg ──
console.log("\n== 视频（TS + JPG 封面）==");
await parseAndFirst(VIDEO, "单个链接");
check("视频任务跑完", await downloadFirst());
await wait(1500);
let files = await waitFiles((f) => f.some((x) => x.full.endsWith(".ts")) && f.some((x) => x.full.endsWith(".jpg")));
console.log("  产物: " + files.map((f) => f.full.replace(SANDBOX, "")).join(" ｜ "));
const ts = files.find((f) => f.full.endsWith(".ts"));
const cover = files.find((f) => f.full.endsWith(".jpg"));
check("视频成品是 .ts", !!ts, ts?.full?.replace(SANDBOX, ""));
check("封面按图片格式转成了 .jpg", !!cover, cover?.full?.replace(SANDBOX, ""));
if (ts) {
  const streams = probe(ts.full);
  console.log("  ffprobe: " + JSON.stringify(streams));
  check("TS 里视频/音频都在", /video/.test(streams) && /audio/.test(streams), streams);
}
if (cover) {
  const head = readFileSync(cover.full).subarray(0, 3).toString("hex");
  check("封面真是 JPEG（ff d8 ff）", head === "ffd8ff", head);
  // 同目录不该再有 webp：源封面就是 webp，转成功才有 .jpg
  check("没有残留的原格式封面", !files.some((f) => /\.webp$/i.test(f.full)));
}

// ── 3. 音频来源：成品 .mp3 ──
console.log("\n== 音频（MP3）==");
rmSync(`${SANDBOX}/downloads`, { recursive: true, force: true });
await parseAndFirst(AUDIO);
check("音频任务跑完", await downloadFirst());
await wait(1500);
files = await waitFiles((f) => f.some((x) => x.full.endsWith(".mp3")));
console.log("  产物: " + files.map((f) => f.full.replace(SANDBOX, "")).join(" ｜ "));
const mp3 = files.find((f) => f.full.endsWith(".mp3"));
check("音频成品是 .mp3", !!mp3, mp3?.full?.replace(SANDBOX, ""));
check("没有留下 .m4a", !files.some((f) => f.full.endsWith(".m4a")), files.map((f) => f.full.replace(SANDBOX, "")).join(" ｜ "));
if (mp3) {
  const streams = probe(mp3.full);
  console.log("  ffprobe: " + JSON.stringify(streams));
  check("ffprobe 认出 mp3", /mp3/.test(streams), streams);
}

// ── 4. 图文：图片落成 .jpg ──
console.log("\n== 图文（JPG 图片）==");
rmSync(`${SANDBOX}/downloads`, { recursive: true, force: true });
await parseAndFirst(OPUS);
check("图文任务跑完", await downloadFirst());
await wait(1500);
files = await waitFiles((f) => f.some((x) => x.full.endsWith(".jpg")));
console.log("  产物: " + files.map((f) => f.full.replace(SANDBOX, "")).join(" ｜ "));
const images = files.filter((f) => /\.(jpg|jpeg|png|webp|gif)$/i.test(f.full));
const allJpg = images.length > 0 && images.every((f) => /\.jpe?g$/i.test(f.full));
check("图文里的图片全是 .jpg", allJpg, `${images.length} 张：${images.map((f) => f.full.split("/").pop()).join(" ")}`);
if (images[0]) {
  const head = readFileSync(images[0].full).subarray(0, 3).toString("hex");
  check("图片真是 JPEG", head === "ffd8ff", head);
}

console.log("\n控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
ws.close(); process.exit(fails ? 1 : 0);
