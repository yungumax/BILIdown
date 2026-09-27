// 长标题压力测试：用真实的热门榜找「标题最长」的视频与「名字最长」的 UP，
// 分别按单个视频与 UP 空间解析，检查工具条/表格有没有溢出、错位、控制台报错
import { writeFileSync } from "node:fs";

const UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0 Safari/537.36";
const popular = await (
  await fetch("https://api.bilibili.com/x/web-interface/popular?ps=50&pn=1", { headers: { "User-Agent": UA, Referer: "https://www.bilibili.com" } })
).json();
const videos = (popular?.data?.list ?? []).map((v) => ({ bvid: v.bvid, title: v.title, owner: v.owner?.name ?? "", mid: v.owner?.mid ?? 0 }));
if (!videos.length) {
  console.log("热门榜没取到数据：" + JSON.stringify(popular).slice(0, 200));
  process.exit(1);
}
const longest = [...videos].sort((a, b) => b.title.length - a.title.length)[0];
const longestOwner = [...videos].sort((a, b) => b.owner.length - a.owner.length)[0];
console.log(`最长标题(${longest.title.length} 字): ${longest.title}  [${longest.bvid}]`);
console.log(`最长UP名(${longestOwner.owner.length} 字): ${longestOwner.owner}  [mid ${longestOwner.mid}]`);

const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map();
let seq = 0;
const errors = [];
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) {
    const cb = pending.get(m.id);
    if (cb) {
      pending.delete(m.id);
      cb(m);
    }
    return;
  }
  if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text.slice(0, 240));
});
const send = (method, params = {}) =>
  new Promise((res, rej) => {
    const id = ++seq;
    pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result)));
    ws.send(JSON.stringify({ id, method, params }));
  });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable");
await send("Log.enable");
await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const health = () =>
  js(`(() => {
    const de = document.documentElement;
    const bar = document.querySelector(".select-bar");
    const title = document.querySelector(".select-title");
    const tag = document.querySelector(".kind-tag");
    const right = document.querySelector(".bar-right");
    const box = (el) => { if (!el) return null; const r = el.getBoundingClientRect(); return { l: Math.round(r.left), r: Math.round(r.right), w: Math.round(r.width), t: Math.round(r.top) }; };
    const t = box(title), b = box(bar), rg = box(right), tg = box(tag);
    const rows = bar ? [...new Set([...bar.children].map((c) => Math.round(c.getBoundingClientRect().top)))].length : 0;
    const overflows = [...document.querySelectorAll(".select-bar *, .batch-table th, .batch-table td, .video-detail *")]
      .filter((el) => el.scrollWidth - el.clientWidth > 2 && getComputedStyle(el).textOverflow !== "ellipsis" && getComputedStyle(el).overflow !== "hidden")
      .slice(0, 6)
      .map((el) => (el.className || el.tagName) + " scrollW=" + el.scrollWidth + " clientW=" + el.clientWidth);
    return JSON.stringify({
      横向溢出: de.scrollWidth > window.innerWidth + 1 ? de.scrollWidth + " > " + window.innerWidth : "无",
      工具条高: b ? Math.round(document.querySelector(".select-bar").getBoundingClientRect().height) : null,
      工具条行数: rows,
      标题: title ? { 宽: t.w, 省略: title.scrollWidth > title.clientWidth + 1, 完整字符数: (title.textContent || "").length } : null,
      标签: tg ? { 距标题: tg.l - t.r, 距左边: Math.round(((tg.l - b.l) / b.w) * 100) + "%" } : null,
      右侧组贴右: rg ? Math.round(b.r - 14 - rg.r) : null,
      未被省略处理的溢出元素: overflows,
      解析控件: [...(document.querySelectorAll(".select-bar button") || [])].map((x) => x.textContent.trim()).filter(Boolean).join(" | "),
    }, null, 1);
  })()`);

const shot = async (name, clip) => {
  const s = await send("Page.captureScreenshot", clip ? { format: "png", clip: { ...clip, scale: 3 } } : { format: "png" });
  writeFileSync("D:/Zcode/BILIdown/tools/" + name, Buffer.from(s.data, "base64"));
};

/** 单独放大拍一下「继续解析 + 箭头」这个按钮，看箭头是否真的在按钮框内 */
const pillShot = async (name) => {
  const clip = await js(`(() => {
    const el = document.querySelector(".parse-split") || document.querySelector(".batch-picker");
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return JSON.stringify({ x: Math.round(r.left) - 6, y: Math.round(r.top) - 6, width: Math.round(r.width) + 12, height: Math.round(r.height) + 12, w: Math.round(r.width), inner: [...el.children].map((c) => Math.round(c.getBoundingClientRect().width)) });
  })()`);
  if (!clip) return;
  const { w, inner, ...box } = JSON.parse(clip);
  console.log(`  ${name}: 容器宽 ${w}，内部 ${JSON.stringify(inner)}`);
  await shot(name, box);
};

async function parseOne(url, mode) {
  await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
  await wait(400);
  await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes(${JSON.stringify(mode === "single" ? "单个视频" : "批量解析")}))?.click()`);
  await wait(300);
  // 批量模式是 textarea，单个视频模式是 input——两种都要写得进去
  await js(`(() => {
    const el = document.querySelector("textarea") || document.querySelector(".card input:not([type='checkbox'])");
    if (!el) return false;
    const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value").set.call(el, ${JSON.stringify(url)});
    el.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await wait(300);
  await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
  for (let i = 0; i < 12; i += 1) {
    await wait(1400);
    if (await js(`!!document.querySelector(".select-bar")`)) break;
  }
  await wait(900);
  const toast = await js(`document.querySelector(".toast")?.textContent.trim() || ""`);
  if (toast) console.log("  提示: " + toast);
}

// 1) 最长标题的视频（单个视频模式：标题会整条进工具条）
await parseOne(`https://www.bilibili.com/video/${longest.bvid}`, "single");
console.log("长标题·单个视频: " + (await health()));
await shot("long-title-single.png");
const bar1 = JSON.parse(await js(`JSON.stringify((() => { const r = document.querySelector(".select-bar").getBoundingClientRect(); return { x: Math.round(r.left), y: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) }; })())`));
await shot("long-title-bar.png", bar1);

// 2) 最长名字的 UP（UP 空间：来源标题会带 UP 名）
await parseOne(`https://space.bilibili.com/${longestOwner.mid}/video`, "batch");
console.log("长UP名·空间: " + (await health()));
await shot("long-title-space.png");
await pillShot("pill-zoom.png");
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
