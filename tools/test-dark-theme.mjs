// 深色主题验收：切到深色、量各层实际渲染色、算文字对比度，并截图
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
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const shot = async (name) => {
  const s = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync("D:/Zcode/BILIdown/tools/" + name, Buffer.from(s.data, "base64"));
};

// 走界面上的主题菜单切到深色（和用户操作一致）
await js(`[...document.querySelectorAll(".theme-button, button")].find((b) => b.getAttribute("aria-haspopup") === "menu" && b.closest(".titlebar"))?.click()`);
await wait(400);
const picked = await js(`(() => {
  const item = [...document.querySelectorAll(".theme-menu button")].find((b) => b.textContent.includes("深色"));
  if (!item) return false;
  item.click();
  return true;
})()`);
console.log("已点「深色」: " + picked);
await wait(900);

console.log(
  await js(`(() => {
    const token = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    const rgb = (css) => { const m = css.match(/\\d+(\\.\\d+)?/g); return m ? m.slice(0, 3).map(Number) : null; };
    const lum = ([r, g, b]) => { const f = (c) => { c /= 255; return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
    const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return ((x + 0.05) / (y + 0.05)).toFixed(2); };
    // 量的都是元素上真实生效的颜色（token 是十六进制，直接解析会算错）
    const elColor = (sel) => { const el = document.querySelector(sel); return el ? getComputedStyle(el).color : null; };
    const bg = rgb(getComputedStyle(document.querySelector(".content")).backgroundColor) || [15, 16, 17];
    const cardEl = document.querySelector(".card") || document.body;
    const cardRgb = rgb(getComputedStyle(cardEl).backgroundColor) || rgb(token("--card"));
    const text = rgb(elColor("body")) || [232, 233, 234];
    const muted = rgb(elColor(".lead, .note, .hint")) || rgb(token("--muted"));
    const faint = rgb(token("--faint"));
    const card = cardRgb;
    const real = (sel) => { const el = document.querySelector(sel); return el ? getComputedStyle(el).backgroundColor : null; };
    return JSON.stringify({
      主题: document.documentElement.dataset.theme,
      app背景: token("--app-bg"),
      侧栏: token("--side-bg"),
      卡片: token("--card"),
      输入框底色: token("--field"),
      分割线: token("--line"),
      实际渲染: { body: real("body"), 侧栏: real(".sidebar"), 卡片区: real(".content") },
      对比度: { 正文_卡片: ratio(text, card), 次要_卡片: ratio(muted, card), 弱化_卡片: ratio(faint, card), 正文_页面: ratio(text, bg) }
    }, null, 1);
  })()`)
);
await shot("dark-theme.png");
// 顺带把解析页也拍下来，看表格与工具条
await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await wait(500);
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await wait(300);
await js(`(() => { const el = document.querySelector("textarea"); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, "https://space.bilibili.com/927587/lists/108434"); el.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await wait(300);
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 12; i += 1) {
  await wait(1400);
  if (await js(`!!document.querySelector(".select-bar")`)) break;
}
await wait(900);
await shot("dark-theme-list.png");
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close();
process.exit(0);
