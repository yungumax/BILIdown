// 批量解析模式下贴单个视频链接：应展开成它所在的合集；单个视频模式只解析这一个。
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map();
let seq = 0;
const errors = [];
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text.slice(0, 200));
});
const send = (method, params = {}) =>
  new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable");
await send("Log.enable");
await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const VIDEO = "https://www.bilibili.com/video/BV1DP41187GN";

const parseIn = async (modeLabel, isBatch) => {
  await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
  await new Promise((r) => setTimeout(r, 500));
  await js(`[...document.querySelectorAll("button")].find(b => b.textContent.trim() === "清空")?.click()`);
  await new Promise((r) => setTimeout(r, 300));
  await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes(${JSON.stringify(modeLabel)}))?.click()`);
  await new Promise((r) => setTimeout(r, 300));
  const sel = isBatch ? "textarea" : "input[spellcheck]";
  await js(`(() => {
    const el = document.querySelector(${JSON.stringify(sel)});
    const proto = el.tagName === "TEXTAREA" ? HTMLTextAreaElement : HTMLInputElement;
    const st = Object.getOwnPropertyDescriptor(proto.prototype, "value").set;
    st.call(el, ${JSON.stringify(VIDEO)});
    el.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await new Promise((r) => setTimeout(r, 300));
  await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
  for (let i = 0; i < 15; i += 1) {
    await new Promise((r) => setTimeout(r, 2000));
    if (await js(`!!document.querySelector(".select-page")`)) break;
  }
  await new Promise((r) => setTimeout(r, 800));
  const state = await js(`JSON.stringify({
    标签: document.querySelector(".kind-tag")?.textContent,
    标题: document.querySelector(".select-title")?.textContent,
    有表格: !!document.querySelector(".batch-table"),
    有详情块: !!document.querySelector(".video-detail"),
    行数: document.querySelectorAll(".batch-table tbody tr").length,
    说明: document.querySelector(".select-page .note")?.textContent || null
  })`);
  console.log(`${modeLabel} → ${state}`);
};

await parseIn("批量解析", true);
await parseIn("单个视频", false);
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
const shot = await send("Page.captureScreenshot", { format: "png" });
const fs = await import("node:fs");
fs.writeFileSync("D:/Zcode/BILIdown/tools/collection-expand.png", Buffer.from(shot.data, "base64"));
ws.close();
process.exit(0);
