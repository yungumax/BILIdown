// 量关键区块的几何：改版前后对比，确保"排版不变"（只有配色/圆角/阴影变化）
import { writeFileSync } from "node:fs";
const out = process.argv[2] || "geometry-before.json";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0;
ws.addEventListener("message", (ev) => { const m = JSON.parse(ev.data); if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } } });
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

// 进解析页并解析一个来源，量输入页与选择页
const geo = async () => JSON.parse(await js(`(() => {
  const box = (sel) => { const el = document.querySelector(sel); if (!el) return null; const r = el.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; };
  return JSON.stringify({
    侧栏: box(".sidebar"),
    主区: box(".content"),
    卡片: box(".content .card"),
    标题: box(".content .card h1") || box(".content .card h2"),
    标签页: box(".tabs"),
    输入框: box("textarea") || box(".card input:not([type=checkbox])"),
    按钮排: box(".actions"),
    支持来源: box(".sources"),
    工具条: box(".select-bar"),
    表格: box(".batch-table"),
    表头: box(".batch-table thead th"),
    首行: box(".batch-table tbody tr"),
    底部统计: box(".results-foot, .table-foot")
  });
})()`));

await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await wait(600);
const input = await geo();
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await wait(300);
await js(`(() => { const el = document.querySelector("textarea"); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, "https://space.bilibili.com/927587/lists/108434?type=season"); el.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await wait(300);
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 14; i += 1) { await wait(1300); if (await js(`!!document.querySelector(".select-bar")`)) break; }
await wait(1200);
const select = await geo();
writeFileSync("D:/Zcode/BILIdown/tools/" + out, JSON.stringify({ 输入页: input, 选择页: select }, null, 1));
console.log("写入 " + out);
console.log("输入页:", JSON.stringify(input));
console.log("选择页:", JSON.stringify(select));
ws.close(); process.exit(0);
