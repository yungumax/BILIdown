// 验收：系列链接必须按系列解析（曾被当成合集，返回别人的 2 条内容）
// 同时验证「不带 ?type=series」的裸链接能靠 mid 核对自动回退到系列
import { writeFileSync } from "node:fs";
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

async function parse(url) {
  await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
  await wait(400);
  await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
  await wait(300);
  await js(`(() => {
    const el = document.querySelector("textarea");
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, ${JSON.stringify(url)});
    el.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await wait(300);
  await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
  for (let i = 0; i < 14; i += 1) {
    await wait(1400);
    if (await js(`!!document.querySelector(".select-bar") || !!document.querySelector(".toast")`)) break;
  }
  await wait(900);
  return await js(`(() => {
    const bar = document.querySelector(".select-bar");
    const rows = [...document.querySelectorAll(".batch-table tbody tr")].filter((tr) => tr.querySelector(".col-check input"));
    return JSON.stringify({
      工具条: !!bar,
      标题: bar?.querySelector(".select-title")?.textContent.trim() ?? null,
      标签: bar?.querySelector(".kind-tag")?.textContent.trim() ?? null,
      计数: bar?.querySelector(".loaded-hint")?.textContent.trim() ?? null,
      行数: rows.length,
      首行: rows[0]?.querySelectorAll("td")[2]?.textContent.trim() ?? null,
      提示: document.querySelector(".toast")?.textContent.trim() ?? ""
    }, null, 1);
  })()`);
}

console.log("① 带 ?type=series 的链接:");
console.log(await parse("https://space.bilibili.com/486287787/lists/90946?type=series"));
await shot("series-fixed.png");

console.log("② 同一个 id 但去掉 ?type=series（应自动识别成系列）:");
console.log(await parse("https://space.bilibili.com/486287787/lists/90946"));

console.log("③ 真合集（不该被改坏）:");
console.log(await parse("https://space.bilibili.com/927587/lists/108434?type=season"));
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
