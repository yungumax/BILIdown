// 下载设置弹层 + 下载全部 + 步骤条状态跟随页面
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
const steps = () => js(`JSON.stringify([...document.querySelectorAll(".steps li")].map(li => ({ 标题: li.querySelector(".title").textContent, 状态: li.className, 有横线: getComputedStyle(li).borderBottomColor !== "rgba(0, 0, 0, 0)" && getComputedStyle(li).borderBottomWidth !== "0px" })))`);

await js(`[...document.querySelectorAll(".sidebar button")].find(b => b.textContent.includes("解析添加")).click()`);
await new Promise((r) => setTimeout(r, 600));
console.log("A) 输入页步骤: " + (await steps()));
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await new Promise((r) => setTimeout(r, 300));
await js(`(() => { const ta = document.querySelector("textarea"); const st = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set; st.call(ta, "https://space.bilibili.com/1858731/favlist?fid=52568231"); ta.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await new Promise((r) => setTimeout(r, 300));
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 12; i += 1) { await new Promise((r) => setTimeout(r, 2000)); if (await js(`!!document.querySelector(".batch-table")`)) break; }
console.log("B) 选择页步骤: " + (await steps()));
console.log("C) 工具条按钮: " + (await js(`JSON.stringify([...document.querySelectorAll(".select-bar button")].map(b => b.textContent.replace(/\s+/g," ").trim()))`)));
console.log("D) 底部只剩: " + (await js(`JSON.stringify(document.querySelector(".select-foot")?.textContent.replace(/\s+/g," ").trim())`)));
await js(`[...document.querySelectorAll(".select-bar button")].find(b => b.textContent.includes("下载设置")).click()`);
await new Promise((r) => setTimeout(r, 400));
console.log("E) 弹层: " + (await js(`(() => { const p = document.querySelector(".dl-pop"); if (!p) return "未打开"; return JSON.stringify({ 项: [...p.querySelectorAll(".pop-field span")].map(s=>s.textContent), 宽度: Math.round(p.getBoundingClientRect().width), 在当前视口内: p.getBoundingClientRect().right <= window.innerWidth }); })()`)));
const shot = await send("Page.captureScreenshot", { format: "png" });
const fs = await import("node:fs");
fs.writeFileSync("D:/Zcode/BILIdown/tools/dl-panel.png", Buffer.from(shot.data, "base64"));
await js(`document.querySelector(".back")?.click()`);
await new Promise((r) => setTimeout(r, 500));
console.log("F) 返回输入页后步骤: " + (await steps()));
console.log("G) 控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
