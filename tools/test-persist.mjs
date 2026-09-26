// 切换一级栏目后解析结果是否保留 + 工具条标题是否完整 + 序号/时长是否居中
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map();
let seq = 0;
const errors = [];
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Runtime.exceptionThrown") errors.push((m.params?.exceptionDetails?.exception?.description || "").slice(0, 150));
});
const send = (method, params = {}) =>
  new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable");
await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;

await js(`[...document.querySelectorAll(".sidebar button")].find(b => b.textContent.includes("解析添加")).click()`);
await new Promise((r) => setTimeout(r, 600));
await js(`(() => { const ta = document.querySelector("textarea"); const st = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set; st.call(ta, "https://www.bilibili.com/bangumi/play/ss39468"); ta.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await new Promise((r) => setTimeout(r, 300));
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 12; i += 1) { await new Promise((r) => setTimeout(r, 2000)); if (await js(`!!document.querySelector(".batch-table")`)) break; }
console.log("1) 番剧标题: " + await js(`JSON.stringify({ 文本: document.querySelector(".select-title")?.textContent, 宽度: Math.round(document.querySelector(".select-title")?.getBoundingClientRect().width || 0), 被截断: (() => { const el = document.querySelector(".select-title"); return el ? el.scrollWidth > el.clientWidth + 1 : null; })() })`));
console.log("2) 列对齐: " + await js(`JSON.stringify([...document.querySelectorAll(".batch-table thead th")].map(t => ({ 列: t.textContent.trim() || "勾选", 对齐: getComputedStyle(t).textAlign })))`));
console.log("3) 勾两行后计数: " + await js(`(() => { const boxes = document.querySelectorAll(".batch-table tbody input"); boxes[0].click(); boxes[1].click(); return "已勾选 2"; })()`));
await new Promise((r) => setTimeout(r, 400));
console.log("   底部: " + await js(`document.querySelector(".select-foot")?.textContent.replace(/\s+/g," ").trim()`));
// 切到设置再切回
await js(`[...document.querySelectorAll(".sidebar button")].find(b => b.textContent.includes("设置")).click()`);
await new Promise((r) => setTimeout(r, 800));
console.log("4) 在设置页时解析页是否隐藏: " + await js(`getComputedStyle(document.querySelector(".select-page")?.closest("div")).display`));
await js(`[...document.querySelectorAll(".sidebar button")].find(b => b.textContent.includes("解析添加")).click()`);
await new Promise((r) => setTimeout(r, 800));
console.log("5) 切回后: " + await js(`JSON.stringify({ 还在选择页: !!document.querySelector(".select-page"), 行数: document.querySelectorAll(".batch-table tbody tr").length, 底部: document.querySelector(".select-foot")?.textContent.replace(/\s+/g," ").trim() })`));
console.log("6) 控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
const shot = await send("Page.captureScreenshot", { format: "png" });
const fs = await import("node:fs");
fs.writeFileSync("D:/Zcode/BILIdown/tools/persist.png", Buffer.from(shot.data, "base64"));
ws.close();
process.exit(0);
