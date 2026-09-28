// 动效体检：节奏 token、各类动画是否真的落到元素上，以及 reduced-motion 是否一键关掉
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0; const errors = [];
ws.addEventListener("message", (ev) => { const m = JSON.parse(ev.data); if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; } if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text.slice(0, 160)); });
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

// 进解析页并解析一个来源，让表格有行
await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
await wait(400);
await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
await wait(300);
await js(`(() => { const el = document.querySelector("textarea"); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, "https://space.bilibili.com/927587/lists/108434?type=season"); el.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await wait(300);
await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
for (let i = 0; i < 14; i += 1) { await wait(1300); if (await js(`!!document.querySelector(".select-bar")`)) break; }
await wait(1200);

const probe = `(() => {
  const cs = getComputedStyle(document.documentElement);
  const anim = (sel) => { const el = document.querySelector(sel); if (!el) return null; const c = getComputedStyle(el); return { name: c.animationName, dur: c.animationDuration, delay: c.animationDelay, transition: c.transitionDuration }; };
  const btn = document.querySelector("button");
  return JSON.stringify({
    节奏: { fast: cs.getPropertyValue("--motion-fast").trim(), base: cs.getPropertyValue("--motion").trim(), ease: cs.getPropertyValue("--ease-out").trim() },
    ready: document.documentElement.classList.contains("ready"),
    栏目淡入: anim(".parse-page, .page-in"),
    结果卡入场: anim(".results"),
    按钮过渡: btn ? getComputedStyle(btn).transitionDuration : null,
    勾选框动画: (() => { const c = document.querySelector('input[type="checkbox"]'); if (!c) return null; c.checked = true; return getComputedStyle(c).animationName; })()
  }, null, 1);
})()`;
console.log("常规偏好: " + (await js(probe)));

// 换成"减少动态效果"：动画关、颜色类过渡保留（位移类被 transition-property 白名单剥离）
await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
await wait(400);
console.log("reduced-motion: " + (await js(`(() => {
  const card = document.querySelector(".results");
  const btn = document.querySelector("button");
  return JSON.stringify({ 结果卡动画: card ? getComputedStyle(card).animationName : null, 按钮过渡属性: btn ? getComputedStyle(btn).transitionProperty : null, 按钮过渡时长: btn ? getComputedStyle(btn).transitionDuration : null });
})()`)));
await send("Emulation.setEmulatedMedia", { features: [] });
console.log("控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
ws.close(); process.exit(0);
