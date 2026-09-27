// 从 iconfont 公共库拉图标，生成 src/icons.js。
// 用法：node tools/fetch-icons.mjs [collectionId]
//   默认 cid=54475（项目在用的那套）。改 PICK 表即可换用库里别的图标。
import { writeFileSync } from "node:fs";

const CID = process.argv[2] || "54475";
const API = `https://www.iconfont.cn/api/collection/detail.json?id=${CID}`;

// 语义名 -> iconfont 里的图标名（改图标只改这张表）
const PICK = [
  ["download", "download"], ["folder", "folder"], ["fileText", "file-text"], ["books", "books"],
  ["bookmark", "bookmark2"], ["link", "link"], ["slidersH", "adjust-horizontal"], ["slidersV", "adjust"],
  ["funnel", "filtering"], ["listDetails", "list-details"], ["listLine", "list-line"], ["checklist", "checklist"],
  ["sortAscending", "sort-ascending"],
  ["refresh", "refresh"], ["reload", "reload"], ["history", "history"], ["undo", "back"],
  ["transfer", "arrows-right-left"], ["chevronDown", "chevron-down"], ["chevronLeft", "chevron-left"],
  ["chevronRight", "chevron-right"], ["play", "play-filled"], ["photo", "photo"], ["album", "album"],
  ["broadcast", "broadcast"], ["server", "server"], ["cloudDownload", "cloud-download"],
  ["info", "info"], ["bell", "bell"], ["star", "star"], ["bulb", "bulb"], ["login", "login"],
  ["check", "check"], ["clipboard", "clipboard-text"], ["edit", "edit"], ["plus", "plus"],
  ["minus", "minus"], ["close", "close"], ["maximize", "maximize"], ["microphone", "microphone"],
  ["user", "user-check"], ["clock", "clock"], ["ruler", "ruler"], ["scan", "scan"],
  ["search", "search"], ["lock", "lock"],
];

const resp = await fetch(API, {
  headers: {
    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/124.0",
    Referer: `https://www.iconfont.cn/collections/detail?cid=${CID}`,
  },
});
if (!resp.ok) throw new Error(`拉取失败: HTTP ${resp.status}`);
const data = await resp.json();
const icons = new Map(((data.data || {}).icons || []).map((i) => [i.name, i]));
console.log(`集合 cid=${CID} 共 ${icons.size} 个图标`);

const pathsOf = (name) => {
  const svg = icons.get(name)?.show_svg || "";
  const out = [];
  for (const m of svg.matchAll(/<path\b([^>]*)\/>/g)) {
    const d = /d="([^"]+)"/.exec(m[1]);
    if (d) out.push({ d: d[1], rule: m[1].includes("evenodd") ? "evenodd" : "" });
  }
  return out;
};

const lines = [
  `// 图标全部取自 iconfont 公共库 cid=${CID}（「线面同构」的一套，1024 视野、填充绘制）。`,
  '// 页面里用 <Icon name="..." />，尺寸由所在位置的 CSS（如 .ghost svg）决定，颜色跟随 currentColor。',
  "// 重新生成：node tools/fetch-icons.mjs（改图标集或换图标库时跑它）。",
  "export const ICONS = {",
];
let missing = 0;
for (const [key, src] of PICK) {
  const paths = pathsOf(src);
  if (!paths.length) { console.log(`  ⚠ 库里没有 ${src}`); missing += 1; continue; }
  const body = paths.map((p) => (p.rule ? `{ d: ${JSON.stringify(p.d)}, rule: "evenodd" }` : `{ d: ${JSON.stringify(p.d)} }`)).join(", ");
  lines.push(`  ${key}: [${body}],  // ${src}`);
}
lines.push("};");
writeFileSync("src/icons.js", `${lines.join("\n")}\n`);
console.log(`写入 src/icons.js：${PICK.length - missing} 个图标${missing ? `，缺 ${missing} 个` : ""}`);
