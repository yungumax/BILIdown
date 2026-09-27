// 图文端到端：解析 /upload/opus → 表格显示 → 勾选 2 条下载 → 检查落盘（图片 + 正文）
import { writeFileSync } from "node:fs";
const URL_OPUS = process.argv[2] || "https://space.bilibili.com/486287787/upload/opus";

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
const shot = async (n) => {
  const s = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync("D:/Zcode/BILIdown/tools/" + n, Buffer.from(s.data, "base64"));
};

await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await wait(400);
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await wait(300);
await js(`(() => {
  const el = document.querySelector("textarea");
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, ${JSON.stringify(URL_OPUS)});
  el.dispatchEvent(new Event("input", { bubbles: true }));
  return true;
})()`);
await wait(300);
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 14; i += 1) {
  await wait(1400);
  if (await js(`!!document.querySelector(".select-bar")`)) break;
}
await wait(900);
console.log(
  "列表: " +
    (await js(`(() => {
    const bar = document.querySelector(".select-bar");
    const rows = [...document.querySelectorAll(".batch-table tbody tr")].filter((tr) => tr.querySelector(".col-check input"));
    const cells = (tr) => [...tr.querySelectorAll("td")].map((td) => td.textContent.trim());
    return JSON.stringify({
      标题: bar?.querySelector(".select-title")?.textContent.trim(),
      标签: bar?.querySelector(".kind-tag")?.textContent.trim(),
      计数: bar?.querySelector(".loaded-hint")?.textContent.trim(),
      行数: rows.length,
      首行: { 序号: cells(rows[0])[1], 标题: cells(rows[0])[2], UP: cells(rows[0])[3], 第四列: cells(rows[0])[4] }
    }, null, 1);
  })()`))
);
await shot("opus-list.png");

// 勾选前两条下载
await js(`[...document.querySelectorAll(".batch-table tbody tr .col-check input")].slice(0, 2).forEach((c) => c.click())`);
await wait(400);
console.log("勾选后按钮: " + (await js(`[...document.querySelectorAll(".select-bar button")].map(b => b.textContent.trim()).filter(Boolean).join(" | ")`)));
await js(`[...document.querySelectorAll(".select-bar button")].find(b => b.textContent.includes("下载所选"))?.click()`);
// 等任务跑完（队列空闲）
for (let i = 0; i < 40; i += 1) {
  await wait(1500);
  const busy = await js(`(() => {
    const rows = [...document.querySelectorAll(".task-row, .task")];
    return rows.some((r) => /下载中|排队|获取/.test(r.textContent));
  })()`);
  if (!busy && i > 1) break;
}
await wait(1200);
console.log(
  "任务: " +
    (await js(`(() => {
    const rows = [...document.querySelectorAll(".task-row, .task")].slice(0, 4);
    return JSON.stringify(rows.map((r) => r.textContent.replace(/\\s+/g, " ").trim().slice(0, 90)), null, 1);
  })()`))
);
await shot("opus-tasks.png");
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
