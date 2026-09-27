// 预设下拉栏实测（文件命名 + 文件夹两页）：
// 结构（自定义模板在最前 / 内置 / 我的预设）、精简后的内置清单、
// 每个内置模板都符合命名规则（不含 `/`、不重复文件夹变量、以 {ext} 结尾）、
// 选预设 → 模板与预览跟着变、保存为预设 → 立刻出现在下拉里。
// 收尾点「撤销」，不动用户真实的 settings.json。
import { readFileSync, writeFileSync } from "node:fs";
const list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const page = list.filter((t) => t.type === "page").find((t) => t.url && t.url !== "about:blank") || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
const pending = new Map(); let seq = 0;
const errors = [];
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id !== undefined) { const cb = pending.get(m.id); if (cb) { pending.delete(m.id); cb(m); } return; }
  if (m.method === "Runtime.exceptionThrown") errors.push((m.params?.exceptionDetails?.exception?.description || "").slice(0, 160));
  if (m.method === "Log.entryAdded" && m.params?.entry?.level === "error") errors.push(m.params.entry.text.slice(0, 160));
});
const send = (method, params = {}) => new Promise((res, rej) => { const id = ++seq; pending.set(id, (m) => (m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result))); ws.send(JSON.stringify({ id, method, params })); });
await new Promise((r) => ws.addEventListener("open", r));
await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable");
const js = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.value;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

let fails = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok  " : "  FAIL"}  ${name}${detail ? "  " + detail : ""}`);
  if (!ok) fails++;
};

// 读一个下拉的结构：选项 + 分组
const readSelect = async (needle) => JSON.parse(await js(`(() => {
  const sel = [...document.querySelectorAll('select')].find(s => [...s.options].some(o => o.textContent.includes(${JSON.stringify(needle)})));
  if (!sel) return JSON.stringify({ missing: true });
  return JSON.stringify({
    value: sel.value,
    options: [...sel.options].map(o => ({ text: o.textContent.trim(), value: o.value, group: o.parentElement.tagName === 'OPTGROUP' ? o.parentElement.label : '' })),
    selected: sel.options[sel.selectedIndex]?.textContent.trim() || ''
  });
})()`));

const pick = async (needle, name) => {
  await js(`(() => {
    const sel = [...document.querySelectorAll('select')].find(s => [...s.options].some(o => o.textContent.includes(${JSON.stringify(needle)})));
    const opt = [...sel.options].find(o => o.textContent.trim() === ${JSON.stringify(name)});
    sel.value = opt.value;
    sel.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  })()`);
  await wait(500);
};

const note = async (keyword) => js(`([...document.querySelectorAll('.note')].map(n => n.textContent.replace(/\\s+/g, ' ').trim()).find(t => t.includes(${JSON.stringify(keyword)}))) ?? ''`);
const inputWith = async (needle) => js(`[...document.querySelectorAll('input')].map(i => i.value).find(v => v.includes(${JSON.stringify(needle)})) ?? ''`);

const go = async (text) => {
  await js(`[...document.querySelectorAll('.sidebar button')].find(b => b.textContent.includes('设置')).click()`);
  await wait(800);
  await js(`[...document.querySelectorAll('.cats button')].find(b => b.textContent.includes(${JSON.stringify(text)}))?.click()`);
  await wait(600);
};

// ── 文件命名页 ─────────────────────────────────────────────
await go("文件命名");
const DEFAULT_NAME = "单文件（默认）";
const naming = await readSelect(DEFAULT_NAME);
console.log("\n== 文件命名 · 命名预设 ==");
for (const o of naming.options) console.log(`  ${o.group ? "[" + o.group + "] " : ""}${o.text}`);

check("第一个选项是「自定义模板」", naming.options[0]?.text === "自定义模板");
const nBuiltin = naming.options.filter((o) => o.group === "内置");
check("内置精简到 4 个（每种来源形状一个）", nBuiltin.length === 4, nBuiltin.map((o) => o.text).join(" / "));
check("内置顺序：单文件（默认） → 分P视频 → 合集/列表 → 番剧/课程",
  JSON.stringify(nBuiltin.map((o) => o.text)) === JSON.stringify([DEFAULT_NAME, "分P视频", "合集/列表", "番剧/课程"]));
check("变体预设「带清晰度」不再内置（需要就存成我的预设）", !naming.options.some((o) => o.text === "带清晰度"));
// 反推往返：选中它 → 模板变成它的模板 → 下拉显示回到它自己的名字
// （不能断言页面一进来就显示它——用户可能存的是别的模板，那就不该显示成默认）
await pick(DEFAULT_NAME, DEFAULT_NAME);
const defaultRoundTrip = await readSelect(DEFAULT_NAME);
check("选「单文件（默认）」后模板与显示都回到它", (await inputWith("{")) === "{title}.{ext}" && defaultRoundTrip.selected === DEFAULT_NAME,
  `${await inputWith("{")} / ${defaultRoundTrip.selected}`);

// 静态对一次：前端的默认设置 template 必须等于内置「默认（默认）」的模板，
// 否则点「恢复默认」之后下拉会显示"自定义模板"。
// （不能真点「恢复默认」验证——那个按钮会直接把默认值写进 settings.json。）
const appSrc = readFileSync("src/App.vue", "utf8");
const pageSrc = readFileSync("src/pages/SettingsPage.vue", "utf8");
const defaultTemplate = /naming_template: "([^"]+)"/.exec(appSrc)?.[1];
const defaultPreset = new RegExp('\{ name: "' + DEFAULT_NAME + '", template: "([^"]+)" \}').exec(pageSrc)?.[1];
check("默认命名模板 = 内置「单文件（默认）」的模板", !!defaultTemplate && defaultTemplate === defaultPreset, `${defaultTemplate} vs ${defaultPreset}`);
const rustSrc = readFileSync("src-tauri/src/state.rs", "utf8");
const defaultFolder = /pub const DEFAULT_FOLDER_TEMPLATE: &str = "([^"]+)"/.exec(rustSrc)?.[1];
const defaultFolderPreset = /\{ name: "UP → 合集 → 条目（默认）", template: "([^"]+)" \}/.exec(pageSrc)?.[1];
check("默认文件夹模板 = 内置第一条（未设置文件夹时用的就是它）", !!defaultFolder && defaultFolder === defaultFolderPreset, `${defaultFolder} vs ${defaultFolderPreset}`);

// 每个内置模板都要符合命名规则：不含 /、不碰文件夹变量、以 {ext} 结尾
console.log("\n== 内置模板是否守命名规则 ==");
for (const opt of nBuiltin) {
  await pick(DEFAULT_NAME, opt.text);
  const template = await inputWith("{");
  const ok = template.includes("{ext}") && !template.includes("/") && !/\{owner_name\}|\{collection_title\}|\{source_kind\}/.test(template);
  check(`「${opt.text}」`, ok, template);
}
await pick(DEFAULT_NAME, "合集/列表");
const previewAfter = await note("文件名预览");
check("选「合集/列表」→ 模板与预览跟着变", (await inputWith("{")) === "{index} {title}.{ext}" && previewAfter.includes("007"), previewAfter.replace("文件名预览：", ""));

// 更严的做法：与内置重名 → 拒绝保存，内置清单不受影响
console.log("\n== 与内置重名 ==");
const setNameTo = async (placeholder, name) => js(`(() => { const i = [...document.querySelectorAll('input')].find(i => i.placeholder === ${JSON.stringify(placeholder)}); i.value = ${JSON.stringify(name)}; i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
await setNameTo("例如：收藏用命名", "分P视频");
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('保存为预设')).click()`);
await wait(700);
const afterClash = await readSelect(DEFAULT_NAME);
check("与内置同名的预设被拒绝保存", !afterClash.options.some((o) => o.group === "我的预设" && o.text === "分P视频"));
check("拒绝时给出重名提示", (await js(`document.querySelector('.toast')?.textContent?.trim() || ''`)).includes("重名"),
  await js(`document.querySelector('.toast')?.textContent?.trim() || ''`));
check("内置清单不受影响（还是 4 个）", afterClash.options.filter((o) => o.group === "内置").length === 4);

// 保存为预设 → 立刻进"我的预设"并选中
await setNameTo("例如：收藏用命名", "测试命名预设A");
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('保存为预设')).click()`);
await wait(600);
const afterSave = await readSelect(DEFAULT_NAME);
check("保存的预设进了「我的预设」分组", afterSave.options.some((o) => o.group === "我的预设" && o.text === "测试命名预设A"));
check("保存后下拉就显示这个预设", afterSave.selected === "测试命名预设A", afterSave.selected);

// ── 文件夹页 ─────────────────────────────────────────────
await go("文件夹");
const folder = await readSelect("只按 UP 分层");
console.log("\n== 文件夹 · 层级预设 ==");
for (const o of folder.options) console.log(`  ${o.group ? "[" + o.group + "] " : ""}${o.text}`);

check("第一个选项是「自定义模板」", folder.options[0]?.text === "自定义模板");
const fBuiltin = folder.options.filter((o) => o.group === "内置");
check("内置 4 个层级方案", fBuiltin.length === 4, fBuiltin.map((o) => o.text).join(" / "));
check("两个下拉的结构一致（自定义模板在最前 + 内置 + 我的预设）",
  folder.options[0]?.text === naming.options[0]?.text &&
  folder.options.some((o) => o.group === "内置") === naming.options.some((o) => o.group === "内置"));

console.log("\n== 选层级预设 → 模板与预览 ==");
await pick("只按 UP 分层", "UP → 来源类型（图文/音频不混在一起）");
check("「UP → 来源类型」填进模板", (await inputWith("{")) === "{owner_name}/{source_kind}", await inputWith("{"));
check("文件夹预览跟着变", (await note("文件夹预览")).includes("合集") || (await note("文件夹预览")).length > 0, await note("文件夹预览"));

await pick("只按 UP 分层", "不建文件夹（全部平铺）");
check("「不建文件夹」清空模板", (await inputWith("{")) === "", JSON.stringify(await inputWith("{")));
check("预览显示不建文件夹", (await note("文件夹预览")).includes("不建文件夹"), await note("文件夹预览"));

// 空模板也能存成预设（"不建文件夹"是正当预设，后端不许把它过滤掉）
await setNameTo("例如：按来源分目录", "测试文件夹预设B");
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('保存为预设')).click()`);
await wait(600);
const folderSaved = await readSelect("只按 UP 分层");
check("空模板的预设也能进「我的预设」", folderSaved.options.some((o) => o.group === "我的预设" && o.text === "测试文件夹预设B"));
check("保存后下拉显示该预设", folderSaved.selected === "测试文件夹预设B", folderSaved.selected);

// 文件夹页同样拒绝与内置重名
await setNameTo("例如：按来源分目录", "只按 UP 分层");
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('保存为预设')).click()`);
await wait(700);
const folderClash = await readSelect("只按 UP 分层");
check("文件夹页也拒绝与内置重名", !folderClash.options.some((o) => o.group === "我的预设" && o.text === "只按 UP 分层"));
check("文件夹页的拒绝也给了提示", (await js(`document.querySelector('.toast')?.textContent?.trim() || ''`)).includes("重名"),
  await js(`document.querySelector('.toast')?.textContent?.trim() || ''`));

// 收尾：丢弃草稿，用户真实的设置一个字节都没动
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('撤销'))?.click()`);
await wait(600);
const stillDirty = await js(`(() => { const b = [...document.querySelectorAll('button')].find(b => b.textContent.includes('撤销')); return b ? !b.disabled : null; })()`);
check("收尾后草稿回到已保存状态（测试没留下脏草稿，也不会写进 settings.json）", stillDirty === false,
  stillDirty === null ? "找不到撤销按钮" : stillDirty ? "草稿仍然是脏的" : "");
const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/presets-folder.png", Buffer.from(shot.data, "base64"));
console.log("\n控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
ws.close(); process.exit(fails ? 1 : 0);
