// 查"命名规则保存后这里没有变化"：预设下拉、模板、预览三者的联动
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map();
let seq = 0;
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } }
});
const send = (method, params = {}) =>
  new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable");
await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;

await js(`[...document.querySelectorAll(".sidebar button")].find(b => b.textContent.includes("设置")).click()`);
await new Promise((r) => setTimeout(r, 700));
await js(`[...document.querySelectorAll(".cat, .layout button")].find(b => b.textContent.includes("文件命名"))?.click()`);
await new Promise((r) => setTimeout(r, 500));

const state = () => js(`(() => {
  const sel = [...document.querySelectorAll("select")].find(s => [...s.options].some(o => o.textContent.includes("分P视频")));
  return JSON.stringify({
    预设下拉值: sel?.value,
    预设下拉显示: sel?.selectedOptions[0]?.textContent.trim(),
    模板: document.querySelector("input[spellcheck]")?.value,
    预览: document.querySelector(".note b")?.textContent,
  });
})()`);

console.log("1) 初始: " + (await state()));
// 选「分P视频」预设
await js(`(() => { const sel = [...document.querySelectorAll("select")].find(s => [...s.options].some(o => o.textContent.includes("分P视频"))); sel.value = "分P视频"; sel.dispatchEvent(new Event("change", { bubbles: true })); return true; })()`);
await new Promise((r) => setTimeout(r, 600));
console.log("2) 选预设后: " + (await state()));
// 手动改模板（模拟"改命名规则"）
await js(`(() => { const el = document.querySelector("input[spellcheck]"); const st = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set; st.call(el, "{title}_{bvid}"); el.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await new Promise((r) => setTimeout(r, 700));
console.log("3) 手改模板后: " + (await state()));
// 保存
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.trim() === "保存").click()`);
await new Promise((r) => setTimeout(r, 1200));
console.log("4) 保存后: " + (await state()));
ws.close();
process.exit(0);
