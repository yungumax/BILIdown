// 明细列表必须在卡内滚动，而不是把整页顶长。
import { writeFileSync } from "node:fs";
import { execSync } from "node:child_process";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0; const errors = [];
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text.slice(0, 200));
});
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;

// 造 12 行重复，让明细足够长
const lines = Array.from({ length: 12 }, () => "https://space.bilibili.com/1858731").join(String.fromCharCode(10));
await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await new Promise((r) => setTimeout(r, 400));
await js(`(() => { const ta = document.querySelector("textarea"); const st = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set; st.call(ta, ${JSON.stringify(lines)}); ta.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await new Promise((r) => setTimeout(r, 300));
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 12; i += 1) { await new Promise((r) => setTimeout(r, 1500)); if (await js(`!!document.querySelector(".skipped-box")`)) break; }
await js(`document.querySelector(".back")?.click()`);
await new Promise((r) => setTimeout(r, 700));

const check = async (label, w, h) => {
  execSync(`powershell -NoProfile -ExecutionPolicy Bypass -File tools/resize-window.ps1 -Width ${w} -Height ${h}`);
  await new Promise((r) => setTimeout(r, 900));
  const state = await js(`(() => {
    const content = document.querySelector(".content");
    const listEl = document.querySelector(".skipped-list");
    const box = document.querySelector(".skipped-box").getBoundingClientRect();
    const card = document.querySelector(".results").getBoundingClientRect();
    return JSON.stringify({
      条目: document.querySelectorAll(".skipped-list li").length,
      列表可视: Math.round(listEl.clientHeight),
      列表内容: Math.round(listEl.scrollHeight),
      列表可内滚: listEl.scrollHeight > listEl.clientHeight + 1,
      列表底边在卡内: Math.round(box.bottom) <= Math.round(card.bottom),
      整页可滚: content.scrollHeight > content.clientHeight + 1,
      整页高度差: content.scrollHeight - content.clientHeight
    });
  })()`);
  console.log(label + ` (${w}x${h}) → ` + state);
  const shot = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(`D:/Zcode/BILIdown/tools/s-${w}x${h}.png`, Buffer.from(shot.data, "base64"));
};

await check("高窗口", 1400, 1300);
await check("标准", 1100, 740);
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
