// 文件夹规则 vs 命名规则 的冲突/重复检查
//
// 规则约定：
//   文件夹规则（设置-文件夹）决定"目录层级"，最终路径 = 文件夹规则 + 命名规则
//   命名规则（设置-文件命名）决定"条目自己叫什么"：视频/音频是文件名，图文/专栏是条目文件夹名
//
// 检查项：
//   ① 两套模板是否用到同一个变量（信息重复，例如 UP 名既在目录又在文件名）
//   ② 命名模板里是否含 /（会额外加一层目录，和文件夹层级重复）
//   ③ 实际渲染出的路径里是否有完全相同的段（标题既当目录又当文件名）
//
// 用法：应用带调试端口启动后 `node tools/test-naming-conflicts.mjs`
import { readFileSync } from "node:fs";

const SETTINGS = "D:/Zcode/_data/bilidown/settings.json";
const DEFAULT_FOLDER = "{owner_name}/{collection_title}";
const varsOf = (tpl) => [...String(tpl ?? "").matchAll(/\{(\w+)\}/g)].map((m) => m[1]);

const settings = JSON.parse(readFileSync(SETTINGS, "utf8"));
const folder = settings.folder_template || DEFAULT_FOLDER;
const naming = settings.naming_template || "{title}";

console.log("文件夹规则（层级）:", folder);
console.log("命名规则（条目）  :", naming);

const problems = [];
const shared = [...new Set(varsOf(folder))].filter((v) => varsOf(naming).includes(v));
if (shared.length) {
  problems.push(`① 两套模板都用到 ${shared.map((v) => `{${v}}`).join("、")} —— 路径里会出现重复信息`);
}
if (String(naming).includes("/")) {
  problems.push("② 命名模板里含 / —— 会再加一层目录，与文件夹层级重复");
}
console.log("模板级检查:", problems.length ? "" : "无重复");
problems.forEach((p) => console.log("  " + p));

const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map();
let seq = 0;
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) {
    const cb = pending.get(m.id);
    if (cb) {
      pending.delete(m.id);
      cb(m);
    }
  }
});
const send = (method, params = {}) =>
  new Promise((res, rej) => {
    const id = ++seq;
    pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result)));
    ws.send(JSON.stringify({ id, method, params }));
  });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable");

const js = async (e) => (await send("Runtime.evaluate", { expression: e, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const CASES = [
  ["合集视频", "https://space.bilibili.com/927587/lists/108434?type=season"],
  ["UP 空间", "https://space.bilibili.com/927587/video"],
  ["图文列表", "https://space.bilibili.com/486287787/upload/opus"],
  ["音频列表", "https://space.bilibili.com/35849261/upload/audio"],
  ["单条图文", "https://www.bilibili.com/opus/1179150912883523593"],
];

let rendered = 0;
let dupesTotal = 0;
for (const [name, url] of CASES) {
  await js(`[...document.querySelectorAll(".steps li")].find(li => li.textContent.includes("解析来源"))?.click()`);
  await wait(400);
  await js(`[...document.querySelectorAll(".tabs button")].find(b => b.textContent.includes("批量解析"))?.click()`);
  await wait(300);
  await js(`(() => { const el = document.querySelector("textarea"); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(el, ${JSON.stringify(url)}); el.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
  await wait(300);
  await js(`[...document.querySelectorAll("button")].find(b => b.textContent.includes("开始解析")).click()`);
  for (let i = 0; i < 15; i += 1) {
    await wait(1300);
    if (await js(`!!document.querySelector(".select-bar")`)) break;
  }
  await wait(2200);
  const rows = JSON.parse(await js(`(() => {
    const rows = Array.from(document.querySelectorAll(".batch-table tbody tr")).filter(tr => tr.querySelector(".col-check input"));
    return JSON.stringify(rows.slice(0, 40).map(tr => String((tr.getAttribute("title") || "").split("文件名：").pop() || "")));
  })()`));
  const dupes = [];
  for (const path of rows) {
    const parts = path.split("/").map((x) => x.replace(/\.(mp4|mkv|m4a)$/, "").trim());
    for (let i = 0; i < parts.length; i += 1) {
      for (let j = i + 1; j < parts.length; j += 1) {
        if (parts[i] && parts[i] === parts[j]) dupes.push(parts[i].slice(0, 26));
      }
    }
  }
  rendered += rows.length;
  dupesTotal += dupes.length;
  console.log(`【${name}】${rows.length} 条${rows.length ? "，例：" + rows[0] : ""}`);
  if (dupes.length) console.log(`   路径里有重复段：${[...new Set(dupes)].join(" / ")}`);
}
console.log(`③ 路径级检查：共看 ${rendered} 条，重复段 ${dupesTotal} 处`);
const ok = problems.length === 0 && dupesTotal === 0;
console.log(ok ? "结论：两套规则无冲突、无重复" : "结论：存在需要修正的重复");
ws.close();
process.exit(ok ? 0 : 1);
