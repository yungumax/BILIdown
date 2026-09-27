// 回答：拉到单次解析上限(300)后下载，再重新解析，会不会重复解析/重复下载？
// 步骤：解析 → 解析全部(拉到上限) → 记下列表摘要 → 重新解析同一链接 → 再解析全部 → 对比两次列表
import { writeFileSync } from "node:fs";
const URL_SPACE = process.argv[2] || "https://space.bilibili.com/927587/video";

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
  if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text.slice(0, 200));
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
const shot = async (name) => {
  const s = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync("D:/Zcode/BILIdown/tools/" + name, Buffer.from(s.data, "base64"));
};

/** 工具条状态 + 表格里所有标题（用标题判断有没有重复行） */
const snapshot = () =>
  js(`(() => {
    const bar = document.querySelector(".select-bar");
    const rows = [...document.querySelectorAll(".batch-table tbody tr")].filter((tr) => tr.querySelector(".col-check input"));
    const titles = rows.map((tr) => tr.querySelectorAll("td")[2]?.textContent.trim() ?? "");
    return JSON.stringify({
      计数: bar?.querySelector(".loaded-hint")?.textContent.trim() ?? null,
      提示: document.querySelector(".note")?.textContent.trim() ?? "",
      按钮: [...(bar?.querySelectorAll("button") ?? [])].map((b) => b.textContent.trim()).filter(Boolean).join(" | "),
      行数: titles.length,
      去重后行数: new Set(titles).size,
      首行: titles[0] ?? null,
      末行: titles[titles.length - 1] ?? null
    }, null, 1);
  })()`);

async function goInput() {
  await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
  await wait(400);
}

async function parseUrl(url) {
  await goInput();
  await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
  await wait(300);
  await js(`(() => {
    const el = document.querySelector("textarea");
    if (!el) return false;
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, ${JSON.stringify(url)});
    el.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await wait(300);
  await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
  for (let i = 0; i < 14; i += 1) {
    await wait(1400);
    if (await js(`!!document.querySelector(".select-bar")`)) break;
  }
  await wait(800);
}

/** 点「解析全部」，一直等到控件消失（= 到底或到上限） */
async function parseAll() {
  await js(`document.querySelector(".parse-split button.arrow")?.click()`);
  await wait(400);
  await js(`[...document.querySelectorAll(".parse-item")].find(b => b.textContent.includes("解析全部"))?.click()`);
  for (let i = 0; i < 60; i += 1) {
    await wait(1500);
    const done = await js(`!document.querySelector(".parse-split")`);
    if (done) break;
  }
  await wait(600);
}

console.log("来源: " + URL_SPACE);
await parseUrl(URL_SPACE);
const first = JSON.parse(await snapshot());
console.log("① 首次解析(第一页): " + JSON.stringify({ 计数: first.计数, 按钮: first.按钮, 行数: first.行数 }));

await parseAll();
const full1 = JSON.parse(await snapshot());
console.log("② 解析全部后: " + JSON.stringify({ 计数: full1.计数, 提示: full1.提示, 按钮: full1.按钮, 行数: full1.行数, 去重后行数: full1.去重后行数 }));
await shot("cap-reparse-1-capped.png");

// 重新解析同一个链接
await parseUrl(URL_SPACE);
const again = JSON.parse(await snapshot());
console.log("③ 重新解析同一链接(第一页): " + JSON.stringify({ 计数: again.计数, 按钮: again.按钮, 行数: again.行数 }));
await parseAll();
const full2 = JSON.parse(await snapshot());
console.log("④ 再解析全部后: " + JSON.stringify({ 计数: full2.计数, 提示: full2.提示, 按钮: full2.按钮, 行数: full2.行数, 去重后行数: full2.去重后行数 }));
await shot("cap-reparse-2-again.png");

console.log("⑤ 两次列表是否完全一致(同样的顺序与内容): " + (full1.首行 === full2.首行 && full1.末行 === full2.末行 && full1.行数 === full2.行数));
console.log("⑥ 第二次列表里有重复行吗: " + (full2.行数 === full2.去重后行数 ? "没有重复" : `重复 ${full2.行数 - full2.去重后行数} 行`));
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
