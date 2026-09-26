// 分组折叠：点分组行收起该来源的行，再点展开；计数与勾选不受影响。
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

const two = ["https://www.bilibili.com/video/BV1Vkag6TExf", "https://space.bilibili.com/1858731"].join(String.fromCharCode(10));
await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await new Promise((r) => setTimeout(r, 500));
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.trim() === "清空")?.click()`);
await new Promise((r) => setTimeout(r, 300));
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await new Promise((r) => setTimeout(r, 300));
await js(`(() => {
  const ta = document.querySelector("textarea");
  const st = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set;
  st.call(ta, ${JSON.stringify(two)});
  ta.dispatchEvent(new Event("input", { bubbles: true }));
  return true;
})()`);
await new Promise((r) => setTimeout(r, 300));
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 15; i += 1) {
  await new Promise((r) => setTimeout(r, 2000));
  if (await js(`!!document.querySelector(".batch-table")`)) break;
}
await new Promise((r) => setTimeout(r, 900));

// 可见行 = offsetParent 不为空的行（v-show 收起的是 display:none）
const visible = () =>
  js(`[...document.querySelectorAll(".batch-table tbody tr")].filter((tr) => !tr.classList.contains("group-row") && tr.offsetParent !== null).length`);
const foldedCount = () => js(`document.querySelectorAll(".group-row.folded").length`);

console.log("1) 初始可见行: " + (await visible()) + "，分组行: " + (await js(`document.querySelectorAll(".group-row").length`)));
await js(`document.querySelectorAll(".group-row")[1].click()`);
await new Promise((r) => setTimeout(r, 400));
console.log("2) 折叠第二个来源: 可见行 " + (await visible()) + "，折叠中 " + (await foldedCount()));
console.log("   该分组行文本: " + (await js(`document.querySelectorAll(".group-row")[1].textContent.replace(/\\s+/g, " ").trim()`)));
console.log("   底部统计未受影响: " + (await js(`document.querySelector(".select-foot")?.textContent.replace(/\\s+/g, " ").trim()`)));
// 折叠时全选，应收起状态仍被选中
await js(`[...document.querySelectorAll(".select-foot button")].find(b => b.textContent.includes("全选已加载"))?.click()`);
await new Promise((r) => setTimeout(r, 400));
console.log("3) 折叠状态下全选: " + (await js(`document.querySelector(".select-foot")?.textContent.replace(/\\s+/g, " ").trim()`)));
await js(`document.querySelectorAll(".group-row")[1].click()`);
await new Promise((r) => setTimeout(r, 400));
console.log("4) 再点展开: 可见行 " + (await visible()) + "，折叠中 " + (await foldedCount()));
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
const shot = await send("Page.captureScreenshot", { format: "png" });
const fs = await import("node:fs");
fs.writeFileSync("D:/Zcode/BILIdown/tools/fold.png", Buffer.from(shot.data, "base64"));
ws.close();
process.exit(0);
