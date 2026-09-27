// 魔法变量面板：栏目横排（一列一组）、列内变量是否被截断、面板是否越出窗口、
// 层级/标签是否与后端 naming::VARIABLES 完全对上、两个面板（命名/文件夹）是否一致。
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

// 读面板：栏目（列）→ 变量；顺带量每列的位置和被截断情况
const READ = `(() => {
  const panel = document.querySelector('.var-panel');
  if (!panel) return JSON.stringify({ missing: true });
  const grid = panel.querySelector('.var-grid');
  const cols = [...grid.querySelectorAll('.var-col')].map(c => {
    const r = c.getBoundingClientRect();
    return {
      name: c.querySelector('.var-col-name')?.textContent.trim(),
      box: [Math.round(r.left), Math.round(r.top), Math.round(r.width)],
      items: [...c.querySelectorAll('.var-item')].map(b => {
        const code = b.querySelector('code'), span = b.querySelector('span');
        return {
          token: (code?.textContent || '').trim().replace(/[{}]/g, ''),
          label: (span?.textContent || '').trim(),
          hint: b.getAttribute('title') || '',
          clipped: code ? code.scrollWidth > code.clientWidth + 1 || span.scrollWidth > span.clientWidth + 1 : null
        };
      })
    };
  });
  const r = panel.getBoundingClientRect();
  return JSON.stringify({
    cols,
    box: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
    grid: { view: Math.round(grid.clientHeight), full: Math.round(grid.scrollHeight) },
    win: [window.innerWidth, window.innerHeight]
  });
})()`;

const openPanel = async (index = 0) => {
  // 只在关着的时候点开：上一次跑完面板可能是开着的，直接 click 会把它点没
  await js(`(() => { if (!document.querySelector('.var-panel')) document.querySelectorAll('.var-picker .ghost')[${index}].click(); return true; })()`);
  await wait(350);
  const tree = JSON.parse(await js(READ));
  await js(`document.querySelectorAll('.var-picker .ghost')[${index}].click()`);
  await wait(250);
  return tree;
};

const flat = (tree) => tree.cols.flatMap(c => c.items.map(i => ({ token: i.token, section: c.name, label: i.label, hint: i.hint })));

const report = (title, tree) => {
  console.log(`\n== ${title} ==`);
  for (const col of tree.cols) {
    console.log(`  ${col.name}  @${col.box[0]},${col.box[1]} 宽${col.box[2]}`);
    for (const i of col.items) console.log(`    {${i.token}}  ${i.label}${i.clipped ? "  ⚠截断" : ""}${i.hint ? `  （悬停：${i.hint}）` : ""}`);
  }
  console.log(`  面板 ${tree.box[2]}×${tree.box[3]} @ left=${tree.box[0]} top=${tree.box[1]}  窗口 ${tree.win[0]}×${tree.win[1]}`);
};

// 进设置 → 文件命名
await js(`[...document.querySelectorAll('.sidebar button')].find(b => b.textContent.includes('设置')).click()`);
await wait(800);
await js(`[...document.querySelectorAll('.cats button')].find(b => b.textContent.includes('文件命名'))?.click()`);
await wait(500);

const naming = await openPanel(0);
report("文件命名 → 命名模板", naming);

const backend = JSON.parse(await js(`(async () => JSON.stringify(await window.__TAURI_INTERNALS__.invoke('naming_variables')))()`));
const rendered = flat(naming).map(({ token, section, label, hint }) => ({ token, section, label, hint }));
// 面板上的悬停说明为空时会退回短标签（鼠标停上去总得有点东西），这里按同样规则比
const expected = backend.map(v => ({ token: v.token, section: v.section, label: v.label, hint: v.hint || v.label }));

console.log("\n[栏目]");
const names = naming.cols.map(c => c.name);
check("八个栏目、顺序对",
  JSON.stringify(names) === JSON.stringify(["标题与作者", "时间", "来源与格式", "视频标识", "分P", "画质与编码", "合集与序号", "剧集信息"]),
  names.join(" / "));

console.log("\n[横向排布]");
const rows = new Map();
for (const [i, col] of naming.cols.entries()) {
  const key = col.box[1];
  if (!rows.has(key)) rows.set(key, []);
  rows.get(key).push({ i, left: col.box[0] });
}
const rowKeys = [...rows.keys()].sort((a, b) => a - b);
check("8 个栏目排成两行", rowKeys.length === 2, `行数=${rowKeys.length} 每行=${[...rows.values()].map(r => r.length).join("/")}`);
check("每行 4 列", [...rows.values()].every(r => r.length === 4));
check("同一行里的列从左到右依次排开",
  [...rows.values()].every(r => r.every((c, k) => k === 0 || c.left > r[k - 1].left)));
check("第二行整体在第一行下面", rowKeys.length === 2 && rowKeys[1] > rowKeys[0]);

console.log("\n[精简]");
const clipped = naming.cols.flatMap(c => c.items.filter(i => i.clipped).map(i => `{${i.token}}`));
check("列内变量与标签都没有被省略号截断", clipped.length === 0, clipped.join(" "));
const longest = Math.max(...naming.cols.flatMap(c => c.items.map(i => i.label.length)));
check("短标签都够短（≤8 字）", longest <= 8, `最长 ${longest} 字`);
check("带歧义的变量有悬停说明", naming.cols.flatMap(c => c.items).filter(i => i.hint).length >= 8,
  `${naming.cols.flatMap(c => c.items).filter(i => i.hint).length} / 19 项有说明`);

console.log("\n[数据]");
check("栏目/短标签/悬停说明与后端 naming_variables 逐项一致", JSON.stringify(rendered) === JSON.stringify(expected),
  `面板 ${rendered.length} 项 / 后端 ${expected.length} 项`);
check("变量不重复出现", new Set(rendered.map(r => r.token)).size === rendered.length && rendered.length === backend.length);

console.log("\n[几何]");
check("面板没有越出窗口左侧", naming.box[0] >= 0, `left=${naming.box[0]}`);
check("面板没有越出窗口右侧", naming.box[0] + naming.box[2] <= naming.win[0], `right=${naming.box[0] + naming.box[2]} / win=${naming.win[0]}`);
check("面板底部在窗口内（默认尺寸下不切掉）", naming.box[1] + naming.box[3] <= naming.win[1], `bottom=${naming.box[1] + naming.box[3]} / win=${naming.win[1]}`);
check("19 个变量一次看全（不再需要滚动）", naming.grid.full <= naming.grid.view + 1, `内容 ${naming.grid.full} / 可视 ${naming.grid.view}`);

// 文件夹页的 + 面板：同一个构建器，栏目应当一模一样
await js(`[...document.querySelectorAll('.cats button')].find(b => b.textContent.includes('文件夹'))?.click()`);
await wait(500);
const folderTree = await openPanel(0);
check("文件夹模板的 + 面板与命名模板一致", JSON.stringify(flat(folderTree)) === JSON.stringify(expected));

const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/var-panel.png", Buffer.from(shot.data, "base64"));
console.log("\n控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
ws.close(); process.exit(fails ? 1 : 0);
