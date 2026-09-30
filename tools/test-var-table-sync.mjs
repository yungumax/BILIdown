// 魔法变量表有两份：Rust 的 naming::VARIABLES（真值）和 api.js 里给浏览器预览用的副本。
// 副本漂了不会报错，只会在脱离桌面壳的预览里悄悄少变量/错分组——这里把它们钉在一起。
import { readFileSync } from "node:fs";

const rust = readFileSync("src-tauri/src/naming.rs", "utf8");
const js = readFileSync("src/api.js", "utf8");

const grab = (text, open, close) => {
  const start = text.indexOf(open);
  if (start < 0) throw new Error(`没找到起点 ${open}`);
  const end = text.indexOf(close, start);
  if (end < 0) throw new Error(`没找到终点 ${close}`);
  return text.slice(start + open.length, end);
};

// Rust: ("token", "说明", "一级", "二级"),
// rustfmt 会把超长元组拆成多行，行内正则会漏抓——按括号段切再从段里挑字符串
const rustRows = [...grab(rust, "pub const VARIABLES: &[(&str, &str, &str, &str)] = &[", "];")
  .matchAll(/\(([^()]*)\)/g)]
  .map((m) => [...m[1].matchAll(/"([^"]*)"/g)].map((x) => x[1]))
  .filter((row) => row.length >= 4)
  .map((row) => row.slice(0, 4));

// JS: ["token", "说明", "一级", "二级"],
const jsRows = [...grab(js, "const VARIABLES = [", "];").matchAll(
  /\[\s*"([^"]+)",\s*"([^"]+)",\s*"([^"]*)",\s*"([^"]*)"\s*\]/g
)].map((m) => [m[1], m[2], m[3], m[4]]);

let fails = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok  " : "  FAIL"}  ${name}${detail ? "  " + detail : ""}`);
  if (!ok) fails++;
};

console.log(`Rust ${rustRows.length} 项 / 预览副本 ${jsRows.length} 项`);
check("两边条数一致", rustRows.length === jsRows.length);
check(
  "标记、短标签、栏目、悬停说明逐行一致（含顺序）",
  JSON.stringify(rustRows) === JSON.stringify(jsRows),
  (() => {
    const bad = rustRows.findIndex((row, i) => JSON.stringify(row) !== JSON.stringify(jsRows[i]));
    return bad < 0 ? "" : `第 ${bad + 1} 行不同：Rust ${JSON.stringify(rustRows[bad])} vs JS ${JSON.stringify(jsRows[bad])}`;
  })()
);
check("每项都有短标签和栏目", rustRows.every(([, label, section]) => label.length > 0 && section.length > 0));

console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
process.exit(fails ? 1 : 0);
