// 音频端到端：解析 /upload/audio → 表格 → 挑最短的一条下载 → 检查落盘
import { writeFileSync } from "node:fs";
const URL_AUDIO = process.argv[2] || "https://space.bilibili.com/649910/upload/audio";

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
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, ${JSON.stringify(URL_AUDIO)});
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
      首行: { 序号: cells(rows[0])[1], 标题: cells(rows[0])[2], UP: cells(rows[0])[3], 时长: cells(rows[0])[4] }
    }, null, 1);
  })()`))
);
await shot("audio-list.png");

// 挑时长最短的一行（用最短的音频做测试，别下 60MB 的长篇）
const pick = await js(`(() => {
  const rows = [...document.querySelectorAll(".batch-table tbody tr")].filter((tr) => tr.querySelector(".col-check input"));
  const dur = (tr) => {
    const t = tr.querySelectorAll("td")[4]?.textContent.trim() ?? "9:99";
    const [m, s] = t.split(":").map(Number);
    return (m || 0) * 60 + (s || 0);
  };
  let best = 0;
  rows.forEach((tr, i) => { if (dur(tr) < dur(rows[best])) best = i; });
  rows[best].querySelector(".col-check input").click();
  return JSON.stringify({ 序号: rows[best].querySelectorAll("td")[1]?.textContent.trim(), 标题: rows[best].querySelectorAll("td")[2]?.textContent.trim(), 时长: rows[best].querySelectorAll("td")[4]?.textContent.trim() });
})()`);
console.log("选中最短的一条: " + pick);
await wait(400);
await js(`[...document.querySelectorAll(".select-bar button")].find(b => b.textContent.includes("下载所选"))?.click()`);
for (let i = 0; i < 60; i += 1) {
  await wait(1500);
  const busy = await js(`/下载中|排队|获取/.test(document.body.textContent)`);
  if (!busy) break;
}
await wait(1500);
console.log("页面提示: " + (await js(`[...document.querySelectorAll(".toast")].map(t => t.textContent.trim()).join(" | ") || "无"`)));
await shot("audio-task.png");
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
