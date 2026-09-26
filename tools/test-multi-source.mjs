// 多来源场景：一次贴两行（一个视频 + 一个收藏夹），应出现来源切换标签，
// 且切换后主体在「视频详情」与「表格」之间切换。
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
  if (m.method === "Runtime.exceptionThrown") {
    errors.push((m.params?.exceptionDetails?.exception?.description || "").slice(0, 150));
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

await js(`document.querySelector(".back")?.click()`);
await new Promise((r) => setTimeout(r, 700));
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await new Promise((r) => setTimeout(r, 400));

const twoLines = [
  "https://www.bilibili.com/video/BV1Vkag6TExf",
  "https://space.bilibili.com/1858731/favlist?fid=52568231",
].join(String.fromCharCode(10));
await js(`(() => {
  const ta = document.querySelector("textarea");
  const st = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set;
  st.call(ta, ${JSON.stringify(twoLines)});
  ta.dispatchEvent(new Event("input", { bubbles: true }));
  return true;
})()`);
await new Promise((r) => setTimeout(r, 400));
console.log("填入的行数: " + (await js(`document.querySelector("textarea").value.split(String.fromCharCode(10)).filter(Boolean).length`)));

await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 15; i += 1) {
  await new Promise((r) => setTimeout(r, 2000));
  if (await js(`document.querySelectorAll(".source-tabs button").length > 1`)) break;
}
console.log("来源标签: " + (await js(`JSON.stringify([...document.querySelectorAll(".source-tabs button")].map(b => b.textContent.trim()))`)));
console.log(
  "当前显示: " +
    (await js(`JSON.stringify({ 标题: document.querySelector(".select-title")?.textContent, 表格: !!document.querySelector(".batch-table"), 详情: !!document.querySelector(".video-detail") })`))
);
await js(`document.querySelectorAll(".source-tabs button")[1]?.click()`);
await new Promise((r) => setTimeout(r, 600));
console.log(
  "切到第 2 个: " +
    (await js(`JSON.stringify({ 标题: document.querySelector(".select-title")?.textContent, 表格: !!document.querySelector(".batch-table"), 详情: !!document.querySelector(".video-detail") })`))
);
await js(`document.querySelectorAll(".source-tabs button")[0]?.click()`);
await new Promise((r) => setTimeout(r, 600));
console.log(
  "切回第 1 个: " +
    (await js(`JSON.stringify({ 标题: document.querySelector(".select-title")?.textContent, 表格: !!document.querySelector(".batch-table"), 详情: !!document.querySelector(".video-detail") })`))
);
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
