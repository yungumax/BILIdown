// 图标名审计：页面里用到的名字必须在 src/icons.js 里真的有 —— 少一个不会报错，
// 只会渲染成一个空 svg（肉眼才看得出来）。这个脚本不需要应用在跑，静态扫就行。
//
//   node tools/test-icons.mjs
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const iconsSource = readFileSync("src/icons.js", "utf8");
const have = new Set([...iconsSource.matchAll(/^  (\w+): \[/gm)].map((m) => m[1]));
// 每个图标至少要有一条 path
const empty = [...iconsSource.matchAll(/^  (\w+): \[\]/gm)].map((m) => m[1]);

const files = [];
const walk = (dir) => {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules") continue;
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full);
    else if (full.endsWith(".vue") || full.endsWith(".js")) files.push(full);
  }
};
walk("src");

const used = new Map(); // 图标名 -> 出处
const add = (name, where) => {
  if (!/^[A-Za-z][A-Za-z0-9]*$/.test(name)) return;
  if (!used.has(name)) used.set(name, where);
};

for (const file of files) {
  const text = readFileSync(file, "utf8");
  // 1) <Icon name="xxx" ...>
  for (const m of text.matchAll(/<Icon[^>]*?\bname="(\w+)"/g)) add(m[1], file);
  // 2) :name="'xxx'" / :name="cond ? 'a' : 'b'"
  for (const m of text.matchAll(/:name="[^"]*?'(\w+)'/g)) add(m[1], file);
  // 3) 图标表：`icon: "xxx"` 与 `xxx: "yyy"`（KIND_ICONS / CATEGORY_ICONS / sources 这类）
  for (const m of text.matchAll(/\bicon:\s*"(\w+)"/g)) add(m[1], file);
  // 4) `const XXX_ICONS = { 键: "图标名" }` 这类表（CATEGORY_ICONS / KIND_ICONS …）
  for (const block of text.matchAll(/const\s+\w*_ICONS\s*=\s*\{([\s\S]*?)\n\};/g)) {
    for (const m of block[1].matchAll(/["']([a-z][A-Za-z0-9]*)["']/g)) add(m[1], file);
  }
}

let fails = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok  " : "  FAIL"}  ${name}${detail ? "  " + detail : ""}`);
  if (!ok) fails++;
};

console.log(`icons.js ${have.size} 个图标；页面用到 ${used.size} 个`);
const missing = [...used.keys()].filter((name) => !have.has(name));
check("页面用到的图标名都在 icons.js 里", missing.length === 0, missing.join(" "));
check("没有空图标（有名字但没有 path）", empty.length === 0, empty.join(" "));

const unused = [...have].filter((name) => !used.has(name));
console.log(`  （未使用的图标 ${unused.length} 个：${unused.join(" ")}）`);
console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
process.exit(fails ? 1 : 0);
