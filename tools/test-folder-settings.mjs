// 文件夹设置验收：导航位置、预设、模板取值、文件夹预览、层级规则说明
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
const js = async (e) => (await send("Runtime.evaluate", { expression: e, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const shot = async (n) => {
  const s = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync("D:/Zcode/BILIdown/tools/" + n, Buffer.from(s.data, "base64"));
};

await js(`[...document.querySelectorAll(".sidebar button, .sidebar li")].find(b => b.textContent.includes("设置"))?.click()`);
await wait(900);
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("层级与文件夹命名"))?.click()`);
await wait(1000);
console.log(
  "文件夹分区: " +
    (await js(`JSON.stringify({
      预设: [...document.querySelectorAll("select option")].map(o => o.textContent.trim()).filter(t => t.includes("UP") || t.includes("平铺") || t.includes("自定义")),
      模板值: [...document.querySelectorAll("input")].map(i => i.value).find(v => v.includes("{")) ?? null,
      预览: ([...document.querySelectorAll(".note")].map(n => n.textContent.replace(/\\s+/g, " ").trim()).find(t => t.includes("文件夹预览"))) ?? null,
      规则说明: ([...document.querySelectorAll(".note")].map(n => n.textContent.replace(/\\s+/g, " ").trim()).find(t => t.includes("层级规则") || t.includes("UP → 合集")))?.slice(0, 80) ?? null
    }, null, 1)`))
);
// 换成"UP → 来源类型"预设，预览应立刻跟着变
await js(`(() => {
  const sel = [...document.querySelectorAll("select")].find(s => [...s.options].some(o => o.textContent.includes("来源类型")));
  if (!sel) return false;
  const opt = [...sel.options].find(o => o.textContent.includes("来源类型"));
  sel.value = opt.value;
  sel.dispatchEvent(new Event("change", { bubbles: true }));
  return true;
})()`);
await wait(1200);
console.log("换预设后预览: " + (await js(`([...document.querySelectorAll(".note")].map(n => n.textContent.replace(/\\s+/g, " ").trim()).find(t => t.includes("文件夹预览"))) ?? "未找到"`)));
await shot("v5-settings-folder.png");
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
