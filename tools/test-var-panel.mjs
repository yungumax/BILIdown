// 魔法变量面板的「二级目录」实测：
// 一级分组 → 二级分组 → 变量 的层级是否真的成形，两个面板（命名/文件夹）是否一致，
// 面板是否越出窗口，以及层级是否与后端 naming::VARIABLES 完全对上。
import { writeFileSync } from "node:fs";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter(t => t.type === "page").find(t => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0;
const errors = [];
ws.addEventListener("message", ev => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Runtime.exceptionThrown") errors.push((m.params?.exceptionDetails?.exception?.description || "").slice(0, 160));
  if (m.method === "Log.entryAdded" && m.params?.entry?.level === "error") errors.push(m.params.entry.text.slice(0, 160));
});
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, m => m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result)); ws.send(JSON.stringify({ id, method, params })); });
await new Promise(r => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable");
const js = async (expr) => (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise(r => setTimeout(r, ms));

let fails = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok  " : "  FAIL"}  ${name}${detail ? "  " + detail : ""}`);
  if (!ok) fails++;
};

// 读面板：按 DOM 顺序还原两级树
const READ = `(() => {
  const panel = document.querySelector('.var-panel');
  if (!panel) return JSON.stringify({ missing: true });
  const grid = panel.querySelector('.var-grid');
  const rows = [];
  let g = null, s = null;
  for (const el of grid.children) {
    if (el.classList.contains('var-heading')) {
      const text = el.textContent.trim();
      const lvl = el.classList.contains('lvl1') ? 1 : el.classList.contains('lvl2') ? 2 : 0;
      if (lvl === 1) { g = { name: text, sections: [], items: [] }; rows.push(g); s = null; }
      else if (lvl === 2) { s = { name: text, items: [] }; (g ? g.sections : rows).push(s); }
      else { rows.push({ name: text, lvl0: true }); }
    } else if (el.classList.contains('var-item')) {
      const token = (el.querySelector('code')?.textContent || '').trim().replace(/[{}]/g, '');
      if (s) s.items.push(token); else if (g) g.items.push(token);
    }
  }
  const r = panel.getBoundingClientRect();
  return JSON.stringify({
    rows,
    box: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
    grid: { view: Math.round(grid.clientHeight), full: Math.round(grid.scrollHeight) },
    win: [window.innerWidth, window.innerHeight]
  });
})()`;

const openPanel = async (label, index = 0) => {
  await js(`document.querySelectorAll('.var-picker .ghost')[${index}].click()`);
  await wait(350);
  const raw = await js(READ);
  await js(`document.querySelectorAll('.var-picker .ghost')[${index}].click()`);
  await wait(250);
  return JSON.parse(raw);
};

const flatten = (tree) => {
  const out = [];
  for (const row of tree.rows) {
    for (const item of row.items) out.push({ token: item, group: row.name, section: "" });
    for (const sec of row.sections) for (const item of sec.items) out.push({ token: item, group: row.name, section: sec.name });
  }
  return out;
};

const report = (title, tree) => {
  console.log(`\n== ${title} ==`);
  for (const row of tree.rows) {
    const own = row.items.length ? ` [${row.items.join(" ")}]` : "";
    console.log(`  ${row.lvl0 ? "?? 旧式单级标题: " : ""}${row.name}${own}`);
    for (const sec of row.sections) console.log(`    · ${sec.name}  [${sec.items.join(" ")}]`);
  }
  console.log(`  面板 ${tree.box[2]}×${tree.box[3]} @ left=${tree.box[0]}  窗口 ${tree.win[0]}×${tree.win[1]}`);
};

// 进设置 → 文件命名
await js(`[...document.querySelectorAll('.sidebar button')].find(b => b.textContent.includes('设置')).click()`);
await wait(800);
await js(`[...document.querySelectorAll('.cats button')].find(b => b.textContent.includes('文件命名'))?.click()`);
await wait(500);

const naming = await openPanel("文件命名", 0);
report("文件命名 → 命名模板", naming);

const backend = JSON.parse(await js(`(async () => JSON.stringify(await window.__TAURI_INTERNALS__.invoke('naming_variables')))()`));
const rendered = flatten(naming);
const expected = backend.map(v => ({ token: v.token, group: v.group, section: v.section }));

console.log("\n[层级]");
const names = naming.rows.map(r => r.name);
check("一级标题就是四个分组、顺序对", JSON.stringify(names) === JSON.stringify(["通用", "视频", "批量来源", "番剧与课程"]), names.join(" / "));
check("没有残留的旧式单级标题", !naming.rows.some(r => r.lvl0));
check("每个一级分组下都有变量", naming.rows.every(r => r.items.length + r.sections.reduce((n, s) => n + s.items.length, 0) > 0));
const sub = (g) => (naming.rows.find(r => r.name === g)?.sections || []).map(s => s.name);
check("通用 的二级目录", JSON.stringify(sub("通用")) === JSON.stringify(["标题与作者", "时间", "来源与格式"]), sub("通用").join(" / "));
check("视频 的二级目录", JSON.stringify(sub("视频")) === JSON.stringify(["视频标识", "分P", "画质与编码"]), sub("视频").join(" / "));
check("小分组不再细分（数据里二级为空就不出标题）", sub("批量来源").length === 0 && sub("番剧与课程").length === 0);

console.log("\n[数据]");
check("两级层级与后端 naming_variables 一致", JSON.stringify(rendered) === JSON.stringify(expected),
  `面板 ${rendered.length} 项 / 后端 ${expected.length} 项`);
check("变量不重复出现", new Set(rendered.map(r => r.token)).size === rendered.length);
check("没有变量掉在一级标题之前", rendered.length === backend.length);

console.log("\n[几何]");
check("面板没有越出窗口左侧", naming.box[0] >= 0, `left=${naming.box[0]}`);
check("面板没有越出窗口右侧", naming.box[0] + naming.box[2] <= naming.win[0], `right=${naming.box[0] + naming.box[2]} / win=${naming.win[0]}`);
check("面板底部在窗口内（默认尺寸下不切掉）", naming.box[1] + naming.box[3] <= naming.win[1], `bottom=${naming.box[1] + naming.box[3]} / win=${naming.win[1]}`);
const scroll = naming.grid;
check("长清单在面板内滚动（不是被裁掉）", scroll.full > scroll.view, `可视 ${scroll.view} < 内容 ${scroll.full}`);

// 文件夹页的 + 面板：同一个 rows 构建器，层级应当一模一样
await js(`[...document.querySelectorAll('.cats button')].find(b => b.textContent.includes('文件夹'))?.click()`);
await wait(500);
const folderTree = await openPanel("文件夹", 0);
check("文件夹模板的 + 面板层级与命名模板一致", JSON.stringify(flatten(folderTree)) === JSON.stringify(expected));

const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/var-panel.png", Buffer.from(shot.data, "base64"));
console.log("\n控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
ws.close(); process.exit(fails ? 1 : 0);
