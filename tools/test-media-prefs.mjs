// 媒体页实测：顶部两个单值下拉（视频清晰度 / 音频质量）+ 下面两张"优先顺序"卡片
// 作为可选的"自定义"。空表时要显示"尚未自定义，使用上方的…"，加行后换成逐个尝试的说明。
// 收尾点「撤销」，不动用户真实的 settings.json。
import { writeFileSync } from "node:fs";
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

const state = async () => JSON.parse(await js(`(() => {
  const labels = [...document.querySelectorAll('.field > label')].map(l => l.textContent.trim());
  const sel = (label) => {
    const field = [...document.querySelectorAll('.field')].find(f => f.querySelector(':scope > label')?.textContent.trim() === label);
    const s = field?.querySelector('select');
    if (!s) return null;
    const r = s.getBoundingClientRect();
    return {
      value: s.value,
      text: s.options[s.selectedIndex]?.textContent.trim(),
      first: s.options[0]?.textContent.trim(),
      box: [Math.round(r.left), Math.round(r.top), Math.round(r.width)]
    };
  };
  const cards = [...document.querySelectorAll('.sub-card')].map(c => {
    const box = c.getBoundingClientRect();
    return {
      title: c.querySelector('.sub-title')?.textContent.trim(),
      rows: c.querySelectorAll('.pref-row').length,
      note: c.querySelector('.note')?.textContent.replace(/\\s+/g, ' ').trim(),
      box: [Math.round(box.left), Math.round(box.top), Math.round(box.width)]
    };
  });
  return JSON.stringify({
    清晰度: sel('视频清晰度'),
    音频: sel('音频质量'),
    卡片: cards,
    标签: labels.slice(0, 6)
  });
})()`));

const clickAdd = async (label) => {
  await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes(${JSON.stringify(label)})).click()`);
  await wait(450);
};
const clearRows = async () => {
  for (const card of [0, 1]) {
    for (let i = 0; i < 12; i++) {
      const left = await js(`[...document.querySelectorAll('.sub-card')][${card}].querySelectorAll('.pref-row').length`);
      if (!left) break;
      // 按 title 找删除键：别用 :last-child（行的动作区结构会变，静默点空过）
      const clicked = await js(`(() => {
        const card = [...document.querySelectorAll('.sub-card')][${card}];
        const btn = card.querySelector('.pref-row button[title="删除"]');
        if (!btn) return false;
        btn.click();
        return true;
      })()`);
      if (!clicked) { console.log("  [提示] 找不到删除按钮，停止清空"); break; }
      await wait(260);
    }
  }
};

// 进设置 → 媒体
await js(`[...document.querySelectorAll('.sidebar button')].find(b => b.textContent.includes('设置')).click()`);
await wait(900);
await js(`[...document.querySelectorAll('.cats button')].find(b => b.querySelector('.label')?.textContent.trim() === '媒体')?.click()`);
await wait(600);

console.log("== 媒体页 ==");
for (const [k, v] of Object.entries(await state())) console.log(`  ${k}: ${JSON.stringify(v)}`);

let s = await state();
check("顶部有「视频清晰度」下拉", !!s.清晰度, JSON.stringify(s.清晰度));
check("顶部有「音频质量」下拉", !!s.音频, JSON.stringify(s.音频));
check("清晰度下拉第一项是最优画质", s.清晰度?.first === "最优画质", s.清晰度?.first);
check("两个下拉并排（同一行、左右各一）",
  s.清晰度?.box?.[1] === s.音频?.box?.[1] && s.清晰度?.box?.[0] < s.音频?.box?.[0] && s.清晰度?.box?.[2] < 600,
  `清晰度 ${JSON.stringify(s.清晰度?.box)} / 音频 ${JSON.stringify(s.音频?.box)}`);
check("两张卡片是「画质优先顺序」与「音频优先顺序」",
  s.卡片.map((c) => c.title).join(" / ") === "画质优先顺序 / 音频优先顺序", s.卡片.map((c) => c.title).join(" / "));

// 清空行 → 应显示"尚未自定义"
await clearRows();
s = await state();
check("清空行后画质卡片显示未自定义", s.卡片[0].rows === 0 && s.卡片[0].note.includes("尚未自定义，使用上方的视频清晰度和编码设置"), s.卡片[0].note);
check("清空行后音频卡片显示未自定义", s.卡片[1].rows === 0 && s.卡片[1].note.includes("尚未自定义，使用上方的音频质量设置"), s.卡片[1].note);

// 加一行 → 说明换成"逐条尝试"
await clickAdd("添加画质");
s = await state();
check("点「添加画质」出现一行", s.卡片[0].rows === 1, `行数 ${s.卡片[0].rows}`);
check("加行后画质说明换成逐条尝试", s.卡片[0].note.includes("逐条尝试"), s.卡片[0].note);
await clickAdd("添加音质");
s = await state();
check("点「添加音质」出现一行", s.卡片[1].rows === 1, `行数 ${s.卡片[1].rows}`);
check("加行后音频说明换成自定义提示（不再是未自定义）", !s.卡片[1].note.includes("尚未自定义") && s.卡片[1].note.includes("退回普通音轨"), s.卡片[1].note);

// 删回空 → 说明回来
await clearRows();
await wait(400);
s = await state();
check("删回空表后两张卡片都回到未自定义",
  s.卡片[0].rows === 0 && s.卡片[1].rows === 0 &&
  s.卡片[0].note.includes("尚未自定义") && s.卡片[1].note.includes("尚未自定义"),
  `行数 ${s.卡片[0].rows}/${s.卡片[1].rows}；${s.卡片[0].note} ｜ ${s.卡片[1].note}`);

// 单值下拉确实写进草稿（改一项再看显示）
await js(`(() => {
  const field = [...document.querySelectorAll('.field')].find(f => f.querySelector(':scope > label')?.textContent.trim() === '视频清晰度');
  const sel = field.querySelector('select');
  const opt = [...sel.options].find(o => o.textContent.includes('480P'));
  sel.value = opt.value; sel.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
})()`);
await wait(400);
s = await state();
check("选 480P 后下拉显示 480P", s.清晰度?.text?.includes("480P"), s.清晰度?.text);

// ── 附加内容已并入媒体 ──
console.log("\n== 附加内容并入媒体 ==");
const cats = await js(`JSON.stringify([...document.querySelectorAll('.cats button .label')].map(l => l.textContent.trim()))`);
console.log("  分类栏: " + cats);
check("「附加内容」这一栏没了", !JSON.parse(cats).includes("附加内容"));
check("其余分类还在", JSON.parse(cats).includes("媒体") && JSON.parse(cats).includes("编码与处理"), cats);

const merged = JSON.parse(await js(`(() => {
  const boxes = [...document.querySelectorAll('.card-check span')].map(s => s.textContent.trim());
  const field = [...document.querySelectorAll('.field')].find(f => f.querySelector(':scope > label')?.textContent.trim() === '下载范围');
  return JSON.stringify({
    复选框: boxes,
    下载范围: field?.querySelector('select')?.options[field.querySelector('select').selectedIndex]?.textContent.trim(),
    范围选项: field ? [...field.querySelector('select').options].map(o => o.textContent.trim()) : []
  });
})()`));
console.log("  搬过来的: " + JSON.stringify(merged));
check("媒体页有「下载封面（独立图片）」", merged.复选框.some((t) => t.includes("下载封面")), JSON.stringify(merged.复选框));
check("「下载字幕」勾选框已摘掉（清单接口被风控挡住）", !merged.复选框.some((t) => t.includes("下载字幕")), JSON.stringify(merged.复选框));
check("媒体页有「下载范围」（两个选项）", merged.范围选项.length === 2 && !!merged.下载范围, merged.范围选项.join(" / "));

// 视频/音频/图片三个格式：同一排、每个至少两个选项、没有 MKV
const formats = JSON.parse(await js(`(() => {
  const cols = [...document.querySelectorAll('.grid3 .field')];
  return JSON.stringify(cols.map(f => {
    const r = f.getBoundingClientRect();
    const sel = f.querySelector('select');
    return { 名: f.querySelector(':scope > label').textContent.trim(), top: Math.round(r.top), left: Math.round(r.left),
             选项: [...sel.options].map(o => o.textContent.trim()) };
  }));
})()`));
console.log("  格式三连排: " + JSON.stringify(formats));
check("三个格式下拉排在同一排", formats.length === 3 && formats.every((f) => f.top === formats[0].top) && formats[0].left < formats[1].left && formats[1].left < formats[2].left);
check("视频格式至少两个选项且没有 MKV",
  formats[0]?.选项.length >= 2 && !formats[0].选项.some((t) => t.includes("MKV")), formats[0]?.选项.join(" / "));
check("音频格式至少两个选项", (formats[1]?.选项.length ?? 0) >= 2, formats[1]?.选项.join(" / "));
check("图片格式至少两个选项", (formats[2]?.选项.length ?? 0) >= 2, formats[2]?.选项.join(" / "));

// 勾选框状态可能是上一条测试留下的：按目标状态点，别盲点（盲点会把它关掉）
const toggleBox = async (label, want) => {
  const state = await js(`(() => {
    const el = [...document.querySelectorAll('.card-check')].find(l => l.textContent.includes(${JSON.stringify(label)}));
    return el ? el.querySelector('input').checked : null;
  })()`);
  if (state === null || state === want) return state !== null;
  await js(`[...document.querySelectorAll('.card-check')].find(l => l.textContent.includes(${JSON.stringify(label)})).querySelector('input').click()`);
  await wait(400);
  return true;
};
await toggleBox("下载封面", true);
check("勾上「下载封面」后说明是独立文件",
  (await js(`[...document.querySelectorAll('.note')].some(n => n.textContent.includes('与视频同名的独立文件'))`)) === true);
await toggleBox("下载弹幕", true);
check("勾上「下载弹幕」后说明是独立 .xml、不合成",
  (await js(`[...document.querySelectorAll('.note')].some(n => n.textContent.includes('不与视频合成'))`)) === true);
await js(`(() => {
  const field = [...document.querySelectorAll('.field')].find(f => f.querySelector(':scope > label')?.textContent.trim() === '下载范围');
  const sel = field.querySelector('select');
  const opt = [...sel.options].find(o => o.textContent.includes('保留原始'));
  sel.value = opt.value; sel.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
})()`);
await wait(400);
check("「下载范围」切到保留原始频道后选中态跟着变",
  (await js(`(() => {
    const field = [...document.querySelectorAll('.field')].find(f => f.querySelector(':scope > label')?.textContent.trim() === '下载范围');
    return field.querySelector('select').selectedOptions[0].textContent.includes('保留原始');
  })()`)) === true);

// 收尾：撤销，别留脏草稿
await js(`[...document.querySelectorAll('button')].find(b => b.textContent.includes('撤销'))?.click()`);
await wait(700);
const stillDirty = await js(`(() => { const b = [...document.querySelectorAll('button')].find(b => b.textContent.includes('撤销')); return b ? !b.disabled : null; })()`);
check("收尾后草稿回到已保存状态", stillDirty === false, stillDirty === null ? "找不到撤销按钮" : stillDirty ? "草稿仍脏" : "");

const shot = await send("Page.captureScreenshot", { format: "png" });
writeFileSync("D:/Zcode/BILIdown/tools/media-prefs.png", Buffer.from(shot.data, "base64"));
console.log("\n控制台错误: " + (errors.length ? errors.join(" | ") : "无"));
console.log(fails ? `\n结果: ${fails} 项不通过` : "\n结果: 全部通过");
ws.close(); process.exit(fails ? 1 : 0);
