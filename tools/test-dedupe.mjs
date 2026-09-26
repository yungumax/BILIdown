// 去重：同一链接两次、同一合集的另一个视频、合集与其中单个视频重叠、UP 空间与其投稿重叠。
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

const run = async (label, inputs) => {
  await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
  await new Promise((r) => setTimeout(r, 500));
  await js(`[...document.querySelectorAll("button")].find(b => b.textContent.trim() === "清空")?.click()`);
  await new Promise((r) => setTimeout(r, 300));
  await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
  await new Promise((r) => setTimeout(r, 300));
  await js(`(() => {
    const ta = document.querySelector("textarea");
    const st = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set;
    st.call(ta, ${JSON.stringify(inputs.join(String.fromCharCode(10)))});
    ta.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await new Promise((r) => setTimeout(r, 300));
  await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
  for (let i = 0; i < 16; i += 1) {
    await new Promise((r) => setTimeout(r, 2000));
    if (await js(`!!document.querySelector(".batch-table")`)) break;
  }
  await new Promise((r) => setTimeout(r, 900));
  const state = await js(`JSON.stringify({
    分组: [...document.querySelectorAll(".group-row")].map((r) => r.textContent.replace(/\\s+/g, " ").trim()),
    行数: document.querySelectorAll(".batch-table tbody tr:not(.group-row)").length,
    去重条数: (document.querySelector(".select-foot")?.textContent.match(/去重 (\\d+)/) || [])[1] || null
  })`);
  console.log(label + " → " + state);
};

// ⓿ 同一个合集里的两个视频（各自都会展开成同一个合集）
const sameColl = await js(`(async () => {
  const p = await window.__TAURI_INTERNALS__.invoke("probe_source", { input: "https://space.bilibili.com/927587/lists/108434", preferCollection: false });
  return p.items.slice(0, 2).map((i) => i.bvid);
})()`);
await run(
  "⓿ 同合集两个视频",
  sameColl.map((b) => `https://www.bilibili.com/video/${b}`)
);

// ① 同一条链接贴两次
await run("① 同链接 ×2", [
  "https://space.bilibili.com/927587/lists/108434",
  "https://space.bilibili.com/927587/lists/108434",
]);
// ② 同一合集里的两个视频（各自都会展开成同一合集）
await run("② 同合集两个视频", [
  "https://space.bilibili.com/927587/lists/108434",
  "https://www.bilibili.com/video/BV1DP41187GN",
]);
// ③ 合集 + 其内的单个视频
await run("③ 合集+其中视频", [
  "https://space.bilibili.com/927587/lists/108434",
  "https://space.bilibili.com/1858731",
  "https://www.bilibili.com/video/BV1XgYC6FEFX",
]);
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
