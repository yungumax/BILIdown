// 校验"只下滑中间部分"：外层内容区不滚、表体内部滚、滚表体时工具条与底部不动
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map();
let seq = 0;
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) {
    const cb = pending.get(m.id);
    if (cb) {
      pending.delete(m.id);
      cb(m);
    }
  }
});
const send = (method, params = {}) =>
  new Promise((res, rej) => {
    const id = ++seq;
    pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result)));
    ws.send(JSON.stringify({ id, method, params }));
  });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable");
await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;

// 解析一个收藏夹，进入表格页
await js(`document.querySelector(".back")?.click()`);
await new Promise((r) => setTimeout(r, 500));
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await new Promise((r) => setTimeout(r, 300));
await js(`(() => {
  const ta = document.querySelector("textarea");
  const st = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set;
  st.call(ta, "https://space.bilibili.com/1858731/favlist?fid=52568231");
  ta.dispatchEvent(new Event("input", { bubbles: true }));
  return true;
})()`);
await new Promise((r) => setTimeout(r, 300));
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 12; i += 1) {
  await new Promise((r) => setTimeout(r, 2000));
  if (await js(`!!document.querySelector(".batch-table")`)) break;
}

const before = await js(`(() => {
  const content = document.querySelector(".content");
  const table = document.querySelector(".table-scroll");
  const bar = document.querySelector(".select-bar");
  const foot = document.querySelector(".select-foot");
  return JSON.stringify({
    外层可滚: content.scrollHeight > content.clientHeight + 1,
    外层 scrollHeight: content.scrollHeight,
    外层 clientHeight: content.clientHeight,
    表格可滚: table.scrollHeight > table.clientHeight + 1,
    表格 scrollHeight: table.scrollHeight,
    表格 clientHeight: table.clientHeight,
    工具条top: Math.round(bar.getBoundingClientRect().top),
    底部top: Math.round(foot.getBoundingClientRect().top),
    底部bottom: Math.round(foot.getBoundingClientRect().bottom),
    视口高: window.innerHeight,
  });
})()`);
console.log("滚动前: " + before);

await js(`document.querySelector(".table-scroll").scrollTop = 260`);
await new Promise((r) => setTimeout(r, 400));
const after = await js(`(() => {
  const table = document.querySelector(".table-scroll");
  return JSON.stringify({
    表格已滚: table.scrollTop,
    工具条top: Math.round(document.querySelector(".select-bar").getBoundingClientRect().top),
    底部top: Math.round(document.querySelector(".select-foot").getBoundingClientRect().top),
    底部bottom: Math.round(document.querySelector(".select-foot").getBoundingClientRect().bottom),
  });
})()`);
console.log("滚动后: " + after);
ws.close();
process.exit(0);
