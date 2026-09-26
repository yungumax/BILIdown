// 多个单视频：应显示成列表（表格 + 勾选 + 下载所选），而不是一个个切标签看详情。
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

// 从收藏夹里取三条视频的 bvid
const bvids = await js(`(async () => {
  const p = await window.__TAURI_INTERNALS__.invoke("probe_source", { input: "https://space.bilibili.com/1858731/favlist?fid=52568231" });
  return p.items.slice(0, 3).map((i) => i.bvid);
})()`);
console.log("三个视频: " + bvids.join(", "));

// 回输入页、清空、批量模式贴三行
await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await new Promise((r) => setTimeout(r, 500));
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.trim() === "清空")?.click()`);
await new Promise((r) => setTimeout(r, 300));
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await new Promise((r) => setTimeout(r, 300));
const lines = bvids.map((b) => `https://www.bilibili.com/video/${b}`).join(String.fromCharCode(10));
await js(`(() => {
  const ta = document.querySelector("textarea");
  const st = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set;
  st.call(ta, ${JSON.stringify(lines)});
  ta.dispatchEvent(new Event("input", { bubbles: true }));
  return true;
})()`);
await new Promise((r) => setTimeout(r, 300));
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 15; i += 1) {
  await new Promise((r) => setTimeout(r, 2000));
  if (await js(`!!document.querySelector(".batch-table")`)) break;
}
await new Promise((r) => setTimeout(r, 1000));
console.log(
  "多视频视图: " +
    (await js(`JSON.stringify({
      表格: !!document.querySelector(".batch-table"),
      详情块: !!document.querySelector(".video-detail"),
      标题: document.querySelector(".select-title")?.textContent,
      标签: document.querySelector(".kind-tag")?.textContent,
      行数: document.querySelectorAll(".batch-table tbody tr").length,
      表头: [...document.querySelectorAll(".batch-table thead th")].map((t) => t.textContent.trim() || "勾选"),
      第一行: [...(document.querySelectorAll(".batch-table tbody tr")[0]?.querySelectorAll("td") || [])].map((td) => td.textContent.trim().slice(0, 22)),
      底部: document.querySelector(".select-foot")?.textContent.replace(/\s+/g, " ").trim(),
      动作行: document.querySelector(".bar-actions")?.textContent.replace(/\s+/g, " ").trim()
    })`))
);
// 勾两行看计数
await js(`(() => { const b = document.querySelectorAll(".batch-table tbody input"); b[0].click(); b[2].click(); return true; })()`);
await new Promise((r) => setTimeout(r, 400));
console.log("勾选后: " + (await js(`JSON.stringify({ 底部: document.querySelector(".select-foot")?.textContent.replace(/\s+/g," ").trim(), 下载所选: [...document.querySelectorAll(".bar-actions button")].find(b=>b.textContent.includes("下载所选"))?.textContent.trim() })`)));
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
const shot = await send("Page.captureScreenshot", { format: "png" });
const fs = await import("node:fs");
fs.writeFileSync("D:/Zcode/BILIdown/tools/multi-video.png", Buffer.from(shot.data, "base64"));
ws.close();
process.exit(0);
