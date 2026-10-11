// 「边框比图大很多」排查：量内容库页所有缩略图/封面盒子 vs 内部图片的实际渲染尺寸。
// 只读探测，不点击不改状态。用法：node tools/probe-border-img.mjs [9411]
const port = process.argv[2] || "9411";
const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0;
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } }
});
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const report = await js(`(async () => {
  const out = { zoom: getComputedStyle(document.documentElement).zoom, inner: [innerWidth, innerHeight], dpr: devicePixelRatio };
  // 切到内容库页（侧栏导航）
  const nav = [...document.querySelectorAll('aside button, nav button, .nav button')].find(b => b.textContent.includes('内容库'));
  if (nav) nav.click();
  await new Promise(r => setTimeout(r, 800));
  const card = [...document.querySelectorAll('.card')].find(c => c.offsetParent !== null);
  out.page = card ? (card.querySelector('.tabs') ? 'library-list' : 'other') : 'not-found';
  // 等图片加载
  await new Promise(r => setTimeout(r, 2500));
  const probe = (sel) => [...card.querySelectorAll(sel)].map((el) => {
    const b = el.getBoundingClientRect();
    const img = el.querySelector('img');
    const cs = getComputedStyle(el);
    if (!img) return { sel, box: [b.width, b.height].map(v => +v.toFixed(1)), img: 'none', bg: cs.backgroundColor, border: cs.borderWidth };
    const r = img.getBoundingClientRect();
    return {
      sel, box: [b.width, b.height].map(v => +v.toFixed(1)), border: cs.borderWidth,
      imgBox: [r.width, r.height].map(v => +v.toFixed(1)),
      natural: img.naturalWidth > 0 ? [img.naturalWidth, img.naturalHeight] : 'NOT-LOADED',
      complete: img.complete, srcTail: (img.currentSrc || img.src || '').split('/').pop().slice(0, 40),
    };
  });
  out.thumbs = probe('.thumb');
  out.covers = probe('.cover');
  return out;
})()`);
console.log(JSON.stringify(report, null, 1));
ws.close();
