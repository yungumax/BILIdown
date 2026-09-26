// 自适应：宽窗口内容限宽居中；窄窗口不横向溢出。
import { writeFileSync } from "node:fs";
import { execSync } from "node:child_process";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map();
let seq = 0;
const errors = [];
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text.slice(0, 200));
});
const send = (method, params = {}) =>
  new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable");
await send("Log.enable");
await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;

const check = async (label, w, h) => {
  execSync(`powershell -NoProfile -ExecutionPolicy Bypass -File tools/resize-window.ps1 -Width ${w} -Height ${h}`);
  await new Promise((r) => setTimeout(r, 900));
  const state = await js(`(() => {
    const content = document.querySelector(".content");
    const root = document.querySelector(".content > *");
    const box = root.getBoundingClientRect();
    var padding = 22;
    var inner = content.clientWidth;
    var centered = Math.abs((box.left - content.getBoundingClientRect().left) - (inner - box.width) / 2) < 6;
    return JSON.stringify({
      内容宽: Math.round(box.width),
      内容区宽: inner,
      横向溢出: content.scrollWidth > inner + 1,
      居中: centered
    });
  })()`);
  console.log(label + ` (${w}x${h}) → ` + state);
  const shot = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(`D:/Zcode/BILIdown/tools/resp-${w}.png`, Buffer.from(shot.data, "base64"));
};

await check("窄窗口", 920, 640);
await check("标准", 1100, 740);
await check("宽窗口", 2000, 1200);
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
