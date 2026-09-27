// 内容库实测（沙箱实例、已登录）：集合列表 → 搜索 → 详情 → 翻页 → 勾选 → 下载所选。
// 用真实账号数据；只读接口，不改任何设置；下载会真入队（沙箱里）。
//   BILIDOWN_COOKIE_FILE='D:\Zcode\_data\bilidown-sandbox\cookies.json' \
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 ./target/release/bilidown.exe
import { readdirSync, readFileSync, writeFileSync } from "node:fs";

const SANDBOX = "D:/Zcode/_data/bilidown-sandbox";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0; const errors = [];
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Runtime.exceptionThrown") errors.push((m.params?.exceptionDetails?.exception?.description || "").slice(0, 150));
  if (m.method === "Log.entryAdded" && m.params?.entry?.level === "error") errors.push(m.params.entry.text.slice(0, 150));
});
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
let fails = 0;
const check = (name, ok, detail = "") => { console.log(`${ok ? "  ok  " : "  FAIL"}  ${name}${detail ? "  " + detail : ""}`); if (!ok) fails++; };

/** 内容库那张卡片（解析页是 v-show 的，别选到它） */
const LIB = `[...document.querySelectorAll('.card')].find(c => c.offsetParent !== null && c.querySelector('.tabs'))`;
/** 入队次数从后端日志数（任务行在"传输"页，本页看不到） */
const enqueued = () => {
  try {
    const logs = readdirSync(`${SANDBOX}/logs`).filter((n) => n.endsWith(".log"));
    return logs
      .map((n) => readFileSync(`${SANDBOX}/logs/${n}`, "utf8"))
      .join("\n")
      .split(/\r?\n/)
      .filter((l) => l.includes("任务入队")).length;
  } catch {
    return 0;
  }
};

await js(`[...document.querySelectorAll('.sidebar button')].find(b => b.textContent.includes('内容库'))?.click()`);
await wait(2200);

console.log("\n== 集合列表 ==");
// 脚本可能接在上一次跑完的状态后面：先回「收藏夹」标签，再退回列表视图
await js(`(() => { const b = [...${LIB}.querySelectorAll('.tabs button')].find(b => b.textContent.includes('收藏夹')); if (b && !b.classList.contains('active')) b.click(); return true; })()`);
await wait(1200);
await js(`(() => { const b = ${LIB}.querySelector('.head.detail .back'); if (b) b.click(); return true; })()`);
await wait(1500);
const tabs = await js(`${LIB}.querySelector('.tabs')?.innerText.replace(/\s+/g, ' ')`);
console.log("  标签: " + tabs);
check("两个标签都带数量", /收藏夹\s*\d+/.test(tabs) && /订阅合集\s*\d+/.test(tabs), tabs);
check("头部说明（共 N 个内容集合）", /共\s*\d+\s*个内容集合/.test(await js(`${LIB}.querySelector('.meta')?.innerText || ''`)));
const cards = JSON.parse(await js(`JSON.stringify([...${LIB}.querySelectorAll('.collection')].map(c => c.innerText.replace(/\\s+/g, ' ').trim()))`));
console.log("  集合: " + JSON.stringify(cards.slice(0, 3)));
check("列出账号的收藏夹", cards.length > 0, cards.length + " 个");
check("卡片带视频数", cards.every((c) => /个视频/.test(c)));
await js(`(() => { const i = ${LIB}.querySelector('.search input'); i.value = '咖啡'; i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
await wait(500);
check("搜索能过滤", (await js(`${LIB}.querySelectorAll('.collection').length`)) === 1, String(await js(`${LIB}.querySelectorAll('.collection').length`)));
await js(`(() => { const i = ${LIB}.querySelector('.search input'); i.value = ''; i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
await wait(400);

console.log("\n== 集合详情 ==");
await js(`[...${LIB}.querySelectorAll('.collection')].find(c => c.innerText.includes('默认收藏夹'))?.click()`);
for (let i = 0; i < 25; i += 1) { await wait(1000); if ((await js(`${LIB}.querySelectorAll('.video').length`)) > 0) break; }
await wait(2500);
const videoCount = await js(`${LIB}.querySelectorAll('.video').length`);
check("视频卡片出来了", videoCount > 0, videoCount + " 张");
check("工具栏显示已加载 / 总数", /已加载 \d+ \/ \d+ 项/.test(await js(`${LIB}.querySelector('.loaded')?.innerText || ''`)), await js(`${LIB}.querySelector('.loaded')?.innerText`));
const widths = JSON.parse(await js(`JSON.stringify([...${LIB}.querySelectorAll('.video img')].slice(0, 8).map(i => i.naturalWidth))`));
check("封面真的加载出来了", widths.filter((w) => w > 0).length >= 6, JSON.stringify(widths));
check("分页显示第 x / y 页", /第 \d+ \/ \d+ 页/.test(await js(`${LIB}.querySelector('.pages')?.innerText.replace(/\s+/g, ' ') || ''`)), await js(`${LIB}.querySelector('.pages')?.innerText.replace(/\s+/g, ' ')`));

const first = await js(`${LIB}.querySelector('.video')?.innerText.replace(/\s+/g, ' ').trim()`);
await js(`(() => { const b = [...${LIB}.querySelectorAll('.pages button')].find(b => b.textContent.trim() === '›'); if (b && !b.disabled) b.click(); return true; })()`);
for (let i = 0; i < 20; i += 1) { await wait(1000); const t = await js(`${LIB}.querySelector('.video')?.innerText.replace(/\s+/g, ' ').trim()`); if (t && t !== first) break; }
check("翻页后内容变了", (await js(`${LIB}.querySelector('.video')?.innerText.replace(/\s+/g, ' ').trim()`)) !== first);
check("翻页后页码是 2", (await js(`${LIB}.querySelector('.page-now')?.innerText`)) === "2");

console.log("\n== 勾选与下载 ==");
await js(`[...${LIB}.querySelectorAll('.pager button')].find(b => b.textContent.includes('全选本页'))?.click()`);
await wait(500);
const nowTotal = await js(`${LIB}.querySelectorAll('.video').length`);
check("全选本页选中整页", (await js(`${LIB}.querySelectorAll('.video.on').length`)) === nowTotal, nowTotal + " 个");
await js(`[...${LIB}.querySelectorAll('.pager button')].find(b => b.textContent.includes('取消本页'))?.click()`);
await wait(400);
// 回到第 1 页再勾：末页往往只有一两条，"点了 2 个" 的期望在末页不成立
await js(`(() => { const b = [...${LIB}.querySelectorAll('.pages button')].find(b => b.textContent.trim() === '«'); if (b && !b.disabled) b.click(); return true; })()`);
for (let i = 0; i < 15; i += 1) { await wait(1000); if ((await js(`${LIB}.querySelectorAll('.video').length`)) > 1) break; }
await js(`[...${LIB}.querySelectorAll('.video')].slice(0, 2).forEach(v => v.click())`);
await wait(400);
const before = enqueued();
const clicked = await js(`(() => { const b = [...${LIB}.querySelectorAll('button')].find(b => b.textContent.includes('下载所选')); if (!b || b.disabled) return 'no/disabled'; b.click(); return 'clicked'; })()`);
await wait(4000);
check("「下载所选」按钮可点", clicked === "clicked", clicked);
check("真的入队了 2 个任务", enqueued() - before === 2, `新增 ${enqueued() - before} 条`);

const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/library-detail.png", Buffer.from(shot.data, "base64"));
console.log("\n控制台错误: " + (errors.join(" | ") || "无"));
console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
ws.close(); process.exit(fails ? 1 : 0);
