// 单条图文/专栏：解析 + 下载（图片 + 正文），专栏走旧链接 read/cv
import { writeFileSync } from "node:fs";
const targets = process.argv.slice(2);
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0; const errors = [];
ws.addEventListener("message", (ev) => { const m = JSON.parse(ev.data); if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; } if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text.slice(0, 200)); });
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const shot = async (n) => { const s = await send("Page.captureScreenshot", { format: "png" }); writeFileSync("D:/Zcode/BILIdown/tools/" + n, Buffer.from(s.data, "base64")); };

for (const url of targets) {
  await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
  await wait(400);
  await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
  await wait(300);
  await js(`(() => { const el = document.querySelector("textarea"); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, ${JSON.stringify(url)}); el.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
  await wait(300);
  await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
  for (let i = 0; i < 14; i += 1) { await wait(1300); if (await js(`!!document.querySelector(".select-bar") || !!document.querySelector(".parsed-chip")`)) break; }
  await wait(900);
  const info = await js(`(() => {
    const bar = document.querySelector(".select-bar");
    const rows = [...document.querySelectorAll(".batch-table tbody tr")].filter(tr => tr.querySelector(".col-check input"));
    const cells = (tr) => [...tr.querySelectorAll("td")].map(td => td.textContent.trim());
    return JSON.stringify({
      标题: bar?.querySelector(".select-title")?.textContent.trim(),
      标签: bar?.querySelector(".kind-tag")?.textContent.trim(),
      计数: bar?.querySelector(".loaded-hint")?.textContent.trim(),
      行数: rows.length,
      首行: rows[0] ? { 标题: cells(rows[0])[2], UP: cells(rows[0])[3], 第四列: cells(rows[0])[4] } : null,
      失败提示: document.querySelector(".item.failed .error")?.textContent.trim() ?? ""
    }, null, 1);
  })()`);
  console.log(`${url}\n  ${info}`);
  await shot("article-" + (url.includes("read/cv") ? "cv" : "opus") + ".png");
}
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close(); process.exit(0);
