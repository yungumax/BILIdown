// 分批加载的端到端验收：一个 1337 条的 UP 空间，上限 300
// 第一批 1–300（解析全部）→ 第二批从第 301 条开始（按序号加载）
// 断言：两批条目不重叠、序号列是来源内的真实序号、计数写明是第几段
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

/** 工具条计数 + 序号列首尾 + 全部标题（标题足以代表条目，用来判重叠） */
const snapshot = () =>
  js(`(() => {
    const bar = document.querySelector(".select-bar");
    const rows = [...document.querySelectorAll(".batch-table tbody tr")].filter((tr) => tr.querySelector(".col-check input"));
    const cells = (tr) => [...tr.querySelectorAll("td")].map((td) => td.textContent.trim());
    const titles = rows.map((tr) => cells(tr)[2]);
    const idx = rows.map((tr) => cells(tr)[1]);
    return JSON.stringify({
      计数: bar?.querySelector(".loaded-hint")?.textContent.trim() ?? null,
      提示: document.querySelector(".note")?.textContent.trim() ?? "",
      按钮: [...(bar?.querySelectorAll("button") ?? [])].map((b) => b.textContent.trim()).filter(Boolean).join(" | "),
      行数: rows.length,
      序号首: idx[0] ?? null,
      序号尾: idx[idx.length - 1] ?? null,
      首行: titles[0] ?? null,
      末行: titles[titles.length - 1] ?? null,
      标题列表: titles
    }, null, 1);
  })()`);

// 解析来源 → 拉满上限
await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await wait(400);
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await wait(300);
await js(`(() => {
  const el = document.querySelector("textarea");
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, ${JSON.stringify(URL_SPACE)});
  el.dispatchEvent(new Event("input", { bubbles: true }));
  return true;
})()`);
await wait(300);
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 14; i += 1) {
  await wait(1400);
  if (await js(`!!document.querySelector(".select-bar")`)) break;
}
await wait(700);
await js(`document.querySelector(".parse-split button.arrow").click()`);
await wait(400);
await js(`[...document.querySelectorAll(".parse-item")].find(b => b.textContent.includes("解析全部")).click()`);
for (let i = 0; i < 60; i += 1) {
  await wait(1400);
  if (await js(`!document.querySelector(".parse-split")`)) break;
}
await wait(600);
const first = JSON.parse(await snapshot());
console.log("第一批: " + JSON.stringify({ 计数: first.计数, 提示: first.提示, 序号: first.序号首 + "–" + first.序号尾, 行数: first.行数 }));
await shot("range-load-1.png");

// 撞上限后入口是否还在（这是原来的缺口：控件一隐藏，剩下的就永远拿不到）
console.log("撞上限后的加载控件: " + (await js(`JSON.stringify([...document.querySelectorAll(".select-bar .parse-split button")].map(b => b.textContent.trim()).filter(Boolean))`)));
await shot("range-load-capped.png");
await js(`document.querySelector(".parse-split button.arrow")?.click()`);
await wait(400);
const hasRange = await js(`!![...document.querySelectorAll(".parse-item")].find(b => b.textContent.includes("按序号加载"))`);
console.log("解析菜单里有「按序号加载」: " + hasRange);
await js(`[...document.querySelectorAll(".parse-item")].find(b => b.textContent.includes("按序号加载")).click()`);
await wait(400);
const prefilled = await js(`document.querySelector(".parse-range input")?.value`);
console.log("预填序号: " + prefilled + "（应为第一批末尾+1 = " + (first.行数 + 1) + "）");
await shot("range-load-menu.png");
await js(`[...document.querySelectorAll(".parse-range button")].find(b => b.textContent.includes("加载")).click()`);
for (let i = 0; i < 30; i += 1) {
  await wait(1200);
  const done = await js(`!document.querySelector(".parse-range input") || !!document.querySelector(".toast")`);
  if (done) break;
}
await wait(800);
const second = JSON.parse(await snapshot());
console.log("第二批: " + JSON.stringify({ 计数: second.计数, 提示: second.提示, 序号: second.序号首 + "–" + second.序号尾, 行数: second.行数 }));
await shot("range-load-2.png");

// 第三批：一键「加载下一批」，应直接接着 601
await js(`[...document.querySelectorAll(".select-bar .parse-split button")].find(b => b.textContent.includes("加载下一批"))?.click()`);
for (let i = 0; i < 30; i += 1) {
  await wait(1200);
  if (!(await js(`[...document.querySelectorAll(".select-bar button")].some(b => b.textContent.includes("加载中"))`))) break;
}
await wait(800);
const third = JSON.parse(await snapshot());
console.log("第三批: " + JSON.stringify({ 计数: third.计数, 序号: third.序号首 + "–" + third.序号尾, 行数: third.行数 }));
const overlap23 = second.标题列表.filter((t) => third.标题列表.includes(t));
console.log("第二/三批重叠条数: " + overlap23.length);
await shot("range-load-3.png");

const overlap = first.标题列表.filter((t) => second.标题列表.includes(t));
console.log("两批重叠条数: " + overlap.length + (overlap.length ? " → " + overlap.slice(0, 3).join(" / ") : "（互不重叠）"));
console.log("序号列是否接着上一批: " + (second.序号首 === "301" ? "是，从 301 开始" : "否：" + second.序号首));
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
