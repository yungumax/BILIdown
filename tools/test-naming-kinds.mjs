// 命名体检：批量视频/图文/音频/单视频 这四类来源，界面预览的文件名是否干净
import { writeFileSync } from "node:fs";
const cases = [
  ["批量视频（合集）", "批量解析", "https://space.bilibili.com/927587/lists/108434?type=season"],
  ["图文列表", "批量解析", "https://space.bilibili.com/486287787/upload/opus"],
  ["音频列表", "批量解析", "https://space.bilibili.com/35849261/upload/audio"],
  ["单个视频", "单个链接", "https://www.bilibili.com/video/BV1j3hd6wE1w"],
  ["单条图文", "单个链接", "https://www.bilibili.com/opus/1179150912883523593"],
];
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0; const errors = [];
ws.addEventListener("message", (ev) => { const m = JSON.parse(ev.data); if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; } if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text.slice(0, 160)); });
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

for (const [name, mode, url] of cases) {
  await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
  await wait(400);
  await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes(${JSON.stringify(mode)}))?.click()`);
  await wait(300);
  await js(`(() => {
    const el = document.querySelector("textarea") || document.querySelector(".card input:not([type=checkbox])");
    const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value").set.call(el, ${JSON.stringify(url)});
    el.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await wait(300);
  await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
  for (let i = 0; i < 14; i += 1) { await wait(1300); if (await js(`!!document.querySelector(".select-bar")`)) break; }
  await wait(1600);
  const out = await js(`(() => {
    const rows = [...document.querySelectorAll(".batch-table tbody tr")].filter(tr => tr.querySelector(".col-check input"));
    const tips = rows.slice(0, 3).map(tr => tr.getAttribute("title") || "").filter(Boolean);
    const single = document.querySelector(".video-detail")?.textContent?.trim().slice(0, 60);
    return JSON.stringify({ 行数: rows.length, 前三条预览: tips, 单条视图: single ?? null }, null, 1);
  })()`);
  console.log(`【${name}】${out}`);
}
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close(); process.exit(0);
