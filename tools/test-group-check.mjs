// 分组选择框：勾它只选中该分组的条目，不影响别的来源，也不触发折叠。
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

const state = async (label) => {
  console.log(
    label +
      " → " +
      (await js(`JSON.stringify({
        分组框: [...document.querySelectorAll(".group-check")].map((c) => ({ 选中: c.checked, 半选: c.indeterminate })),
        底部: document.querySelector(".select-foot")?.textContent.replace(/\\s+/g, " ").trim(),
        第一组已选: [...document.querySelectorAll(".batch-table tbody tr:not(.group-row)")].slice(0, 1).filter((tr) => tr.classList.contains("on")).length,
        折叠中: document.querySelectorAll(".group-row.folded").length
      })`))
  );
};

await state("1) 初始");
// 勾第一组（1 条）
await js(`document.querySelectorAll(".group-check")[0].click()`);
await new Promise((r) => setTimeout(r, 400));
await state("2) 勾第一组");
// 勾第二组（100 条）
await js(`document.querySelectorAll(".group-check")[1].click()`);
await new Promise((r) => setTimeout(r, 400));
await state("3) 再勾第二组");
// 取消第一组
await js(`document.querySelectorAll(".group-check")[0].click()`);
await new Promise((r) => setTimeout(r, 400));
await state("4) 取消第一组");
// 半选：取消第二组里的一行
await js(`document.querySelectorAll(".batch-table tbody tr:not(.group-row) input")[1].click()`);
await new Promise((r) => setTimeout(r, 400));
await state("5) 取消其中一行（应半选）");
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
const shot = await send("Page.captureScreenshot", { format: "png" });
const fs = await import("node:fs");
fs.writeFileSync("D:/Zcode/BILIdown/tools/group-check.png", Buffer.from(shot.data, "base64"));
ws.close();
process.exit(0);
