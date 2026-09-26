// 「选择内容」独立页 + 增量加载实测
import { writeFileSync } from "node:fs";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter(t => t.type === "page").find(t => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0; const errors = [];
ws.addEventListener("message", ev => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Runtime.exceptionThrown") errors.push((m.params?.exceptionDetails?.exception?.description || "").slice(0, 200));
});
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, m => m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result)); ws.send(JSON.stringify({ id, method, params })); });
await new Promise(r => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;

await js(`[...document.querySelectorAll(".sidebar button")].find(b => b.textContent.includes("解析添加")).click()`);
await new Promise(r => setTimeout(r, 600));
await js(`(() => { const ta = document.querySelector("textarea"); const s = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set; s.call(ta, "https://space.bilibili.com/1858731/favlist?fid=52568231"); ta.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await new Promise(r => setTimeout(r, 300));
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 10; i++) {
  await new Promise(r => setTimeout(r, 3000));
  if (await js(`!!document.querySelector(".select-page")`)) break;
}
console.log("1) 是否进入独立页: " + await js(`!!document.querySelector(".select-page")`));
console.log("2) 顶部: " + await js(`JSON.stringify({
  title: document.querySelector(".select-title")?.textContent.trim(),
  tag: document.querySelector(".kind-tag")?.textContent.trim(),
  loaded: document.querySelector(".loaded-hint")?.textContent.replace(/\s+/g," ").trim(),
  rows: document.querySelectorAll(".batch-table tbody tr").length,
  first: document.querySelector(".batch-table .col-title")?.textContent.slice(0, 20),
  footer: document.querySelector(".select-foot .faint")?.textContent.replace(/\s+/g," ").trim()
})`));

// 继续解析
const beforeRows = await js(`document.querySelectorAll(".batch-table tbody tr").length`);
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("继续解析")).click()`);
for (let i = 0; i < 10; i++) {
  await new Promise(r => setTimeout(r, 800));
  if (await js(`document.querySelectorAll(".batch-table tbody tr").length`) > beforeRows) break;
}
console.log("3) 继续解析后行数: " + beforeRows + " → " + await js(`document.querySelectorAll(".batch-table tbody tr").length`));
console.log("   顶部已加载: " + await js(`document.querySelector(".loaded-hint")?.textContent.replace(/\s+/g," ").trim()`));

// 全选已加载 → 下载所选计数
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("全选已加载")).click()`);
await new Promise(r => setTimeout(r, 400));
console.log("4) 全选后: " + await js(`JSON.stringify({ checked: document.querySelectorAll(".batch-table tbody input:checked").length, foot: document.querySelector(".select-foot .faint")?.textContent.replace(/\s+/g," ").trim(), dl: [...document.querySelectorAll("button")].find(b => b.textContent.includes("下载所选"))?.textContent.trim() })`));
// 取消第一行
await js(`document.querySelector(".batch-table tbody input").click()`);
await new Promise(r => setTimeout(r, 300));
console.log("5) 取消一行后下载按钮: " + await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("下载所选"))?.textContent.trim()`));

const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/select-page.png", Buffer.from(shot.data, "base64"));
console.log("6) 控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close(); process.exit(0);
