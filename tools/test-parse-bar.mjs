// 校验工具条：每批下拉保留箭头、「解析」下拉两种批量解析、【下载全部】只在到上限/解析完全时出现
import { writeFileSync } from "node:fs";
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
  if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text.slice(0, 240));
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
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const shot = async (name) => {
  const s = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync("D:/Zcode/BILIdown/tools/" + name, Buffer.from(s.data, "base64"));
};

const state = () =>
  js(`(() => {
    const bar = document.querySelector(".select-bar");
    if (!bar) return JSON.stringify({ 在工具条: false });
    const batch = bar.querySelector(".batch-picker .ghost.compact");
    const split = bar.querySelector(".parse-split");
    return JSON.stringify({
      在工具条: true,
      计数: bar.querySelector(".loaded-hint")?.textContent.trim(),
      每批按钮: batch ? batch.textContent.trim() : null,
      每批带箭头: batch ? !!batch.querySelector("svg.caret") : null,
      解析按钮: split ? [...split.querySelectorAll("button")].map((b) => b.textContent.trim()).filter(Boolean).join(" | ") : null,
      解析带箭头: split ? !!split.querySelector("button.arrow svg.caret") : null,
      下载全部: [...bar.querySelectorAll("button")].some((b) => b.textContent.trim() === "下载全部"),
      下载所选: [...bar.querySelectorAll("button")].map((b) => b.textContent.trim()).find((t) => t.startsWith("下载所选")) || null,
    });
  })()`);

// 进入选择页并解析一个大于一页的合集
await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await wait(400);
await js(`(() => { const ta = document.querySelector("textarea"); const st = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set; st.call(ta, "https://space.bilibili.com/927587/lists/108434"); ta.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await wait(300);
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 14; i += 1) {
  await wait(1500);
  if (await js(`!!document.querySelector(".batch-table")`)) break;
}
await wait(900);
console.log("未加载完: " + (await state()));

// 每批菜单仍可用
await js(`document.querySelector(".batch-picker .ghost.compact").click()`);
await wait(400);
console.log("每批菜单: " + (await js(`JSON.stringify([...document.querySelectorAll(".batch-item")].map(b => b.textContent.trim()))`)));
await shot("parse-bar-menu.png");
await js(`document.querySelector(".batch-picker .ghost.compact").click()`);
await wait(300);

// 解析下拉：两个批量做法
await js(`document.querySelector(".parse-split button.arrow").click()`);
await wait(400);
console.log("解析菜单: " + (await js(`JSON.stringify([...document.querySelectorAll(".parse-item")].map(b => b.textContent.trim()))`)));
console.log("解析菜单带图标: " + (await js(`[...document.querySelectorAll(".parse-item")].every(b => !!b.querySelector("svg"))`)));
await shot("parse-bar-dropdown.png");
await js(`[...document.querySelectorAll(".parse-item")].find(b => b.textContent.includes("解析全部")).click()`);
for (let i = 0; i < 20; i += 1) {
  await wait(1200);
  const done = await js(`(() => { const b = document.querySelector(".parse-split button.seg"); return !b || b.textContent.includes("继续解析"); })()`);
  if (done) break;
}
await wait(700);
console.log("解析全部之后: " + (await state()));
console.log("提示: " + (await js(`document.querySelector(".results .note, .note")?.textContent.trim() || "无"`)));
await shot("parse-bar-done.png");
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
