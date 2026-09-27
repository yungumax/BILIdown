// 设置页冒烟：新增的「单次解析上限」渲染出来、默认值正确、整页无控制台报错
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
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("设置"))?.click()`);
await wait(300);
await js(`[...document.querySelectorAll("button, a, li")].find(b => b.textContent.trim().startsWith("设置"))?.click()`);
await wait(900);
console.log(
  await js(`(() => {
    const label = [...document.querySelectorAll("label")].find((l) => l.textContent.includes("单次解析上限"));
    const select = label?.parentElement?.querySelector("select");
    return JSON.stringify({
      有这一项: !!select,
      当前值: select?.value ?? null,
      选项: select ? [...select.options].map((o) => o.textContent.trim()) : []
    }, null, 1);
  })()`)
);
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
