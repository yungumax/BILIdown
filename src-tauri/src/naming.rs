//! 文件名模板渲染。
//!
//! 界面上的「魔法变量」面板由 [`VARIABLES`] 生成，渲染也用它判断标记是否已知——
//! 两边同一份真值，不会再出现"面板列了、后端不认"的变量（曾经就踩过：
//! 面板列了 18 个，后端只实现了 4 个，其余原样留在文件名里）。

use std::path::PathBuf;

/// 面板里列出的变量：`(标记, 说明)`。顺序即面板顺序。
pub const VARIABLES: &[(&str, &str)] = &[
    ("title", "视频或条目标题"),
    ("part_title", "分P标题"),
    ("part_index", "分P序号"),
    ("bvid", "BV号"),
    ("aid", "AV号"),
    ("cid", "CID"),
    ("owner_name", "UP主名称"),
    ("owner_mid", "UP主MID"),
    ("series_title", "番剧/课程/系列名"),
    ("episode_index", "集序号"),
    ("episode_title", "集标题"),
    ("collection_title", "合集名"),
    ("index", "列表序号"),
    ("quality", "清晰度"),
    ("codec", "编码"),
    ("date", "下载日期（任务创建日）"),
    ("publish_date", "发布时间（B站发布日期）"),
    ("ext", "扩展名"),
];

/// 一次渲染需要的全部取值。取不到的留空——空值在文件名里直接消失，
/// 不会留下 `{episode_title}` 这种字面标记。
#[derive(Debug, Clone, Default)]
pub struct NamingContext {
    pub title: String,
    pub part_title: String,
    pub part_index: u32,
    pub bvid: String,
    pub aid: u64,
    pub cid: u64,
    pub owner_name: String,
    pub owner_mid: u64,
    pub series_title: String,
    pub episode_index: u32,
    pub episode_title: String,
    pub collection_title: String,
    pub index: u32,
    pub quality: String,
    pub codec: String,
    pub date: String,
    pub publish_date: String,
}

impl NamingContext {
    /// 预览用的示例取值：设置页展示模板效果时用，每个变量都给一个能看出差别的值。
    /// 日期类由调用方按本地时区覆盖。
    pub fn sample() -> Self {
        Self {
            title: "示例视频".to_string(),
            part_title: "分P标题".to_string(),
            part_index: 1,
            bvid: "BV1xx411c7mD".to_string(),
            aid: 12345,
            cid: 67890,
            owner_name: "示例UP主".to_string(),
            owner_mid: 1234567,
            series_title: "示例系列".to_string(),
            episode_index: 3,
            episode_title: "第 3 集".to_string(),
            collection_title: "示例合集".to_string(),
            index: 7,
            quality: "1080P60".to_string(),
            codec: "AVC".to_string(),
            date: String::new(),
            publish_date: String::new(),
        }
    }

    /// 取标记的值；`None` 表示这个标记未知（未知标记原样保留，便于发现拼错）。
    fn value(&self, token: &str) -> Option<String> {
        let number = |n: u64| if n == 0 { String::new() } else { n.to_string() };
        let sequence = |n: u32| if n == 0 { String::new() } else { n.to_string() };
        Some(match token {
            "title" => self.title.clone(),
            "part_title" => self.part_title.clone(),
            "part_index" => sequence(self.part_index),
            "bvid" => self.bvid.clone(),
            "aid" => number(self.aid),
            "cid" => number(self.cid),
            "owner_name" => self.owner_name.clone(),
            "owner_mid" => number(self.owner_mid),
            "series_title" => self.series_title.clone(),
            "episode_index" => sequence(self.episode_index),
            "episode_title" => self.episode_title.clone(),
            "collection_title" => self.collection_title.clone(),
            "index" => sequence(self.index),
            "quality" => self.quality.clone(),
            "codec" => self.codec.clone(),
            "date" => self.date.clone(),
            "publish_date" => self.publish_date.clone(),
            _ => return None,
        })
    }
}

/// 渲染模板。
///
/// - `/`（或 `\`）是目录分隔符，每段单独清理非法字符
/// - 模板里写了 `{ext}` 就用模板的位置，没写则给最后一段补上扩展名
/// - 未知标记原样保留；已知但为空的值直接消失；空段与 `.` `..` 丢弃
pub fn render(template: &str, ctx: &NamingContext, ext: &str) -> PathBuf {
    let mut used_ext = false;
    let mut segments: Vec<String> = Vec::new();

    for raw in template.split(['/', '\\']) {
        let (text, had_ext) = substitute(raw, ctx, ext);
        used_ext |= had_ext;
        if let Some(segment) = sanitize_segment(&text) {
            segments.push(segment);
        }
    }

    if !used_ext {
        match segments.last_mut() {
            Some(last) => last.push_str(&format!(".{ext}")),
            None => segments.push(format!("video.{ext}")),
        }
    }

    // 音频/图文没有"分集"：模板里只由分集占位组成的段落会空掉（如 "P - .m4a"）。
    // 这种空壳段落整段丢掉，扩展名并到上一段，别让文件名变成 "P - .m4a"。
    if segments.len() > 1 {
        let suffix = format!(".{ext}");
        let mut merged: Vec<String> = Vec::with_capacity(segments.len());
        for segment in segments {
            let stem = segment.strip_suffix(&suffix).unwrap_or(&segment);
            if has_content(stem) {
                merged.push(segment);
            } else if let Some(prev) = merged.last_mut() {
                prev.push_str(&suffix);
            }
        }
        // 全是空壳时兜底，别产出空路径
        segments = if merged.is_empty() {
            vec![format!("video.{ext}")]
        } else {
            merged
        };
    }

    let mut path = PathBuf::new();
    for segment in &segments {
        path.push(segment);
    }
    path
}

/// 目录名：图文这类"一个条目一个文件夹"的输出用它。
///
/// 和文件名的区别只是不补扩展名——`render` 在没用到 `{ext}` 时会补上
/// `.` + ext，这里传空扩展名会留下一个尾巴点，去掉即可。
pub fn render_dir(template: &str, ctx: &NamingContext) -> PathBuf {
    let path = render(template, ctx, "");
    let mut trimmed = PathBuf::new();
    for segment in path.components() {
        let text = segment.as_os_str().to_string_lossy().to_string();
        let text = text.trim_end_matches('.').trim().to_string();
        // 图文没有"分集"，像 `P{part_index} - {part_title}` 这种只依赖分集的段落
        // 会替换成 "P - "，这种空壳段落直接丢掉，别生成 `标题/P - /`
        if !text.is_empty() && has_content(&text) {
            trimmed.push(text);
        }
    }
    if trimmed.as_os_str().is_empty() {
        trimmed.push("图文");
    }
    trimmed
}

/// 段落里除开头的 P 与标点外，还有没有真内容（中文/字母/数字）。
fn has_content(segment: &str) -> bool {
    let body = segment.trim_start_matches(['P', 'p']).trim();
    body.chars().any(|c| c.is_alphanumeric() || !c.is_ascii())
}

/// 替换一段里的 `{标记}`；同时报告这段是否用到了 `{ext}`。
fn substitute(segment: &str, ctx: &NamingContext, ext: &str) -> (String, bool) {
    let mut out = String::new();
    let mut used_ext = false;
    let mut rest = segment;

    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find('}') {
            Some(end) => {
                let token = &after[1..end];
                if token == "ext" {
                    used_ext = true;
                    out.push_str(ext);
                } else {
                    match ctx.value(token) {
                        Some(value) => out.push_str(&value),
                        None => {
                            // 未知标记原样保留，拼错时一眼能看出来
                            out.push_str(&after[..=end]);
                        }
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    (out, used_ext)
}

/// 清理单个路径段：非法字符换成 `_`，去首尾空白与结尾的点，限长 120。
/// 空段与 `.` `..` 返回 `None`（丢弃，顺带挡掉目录穿越）。
fn sanitize_segment(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if r#"\/:*?"<>|"#.contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return None;
    }
    let mut cleaned = cleaned.to_string();
    if cleaned.chars().count() > 120 {
        cleaned = cleaned.chars().take(120).collect();
    }
    Some(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> NamingContext {
        NamingContext {
            title: "标题".to_string(),
            part_title: "分P标题".to_string(),
            part_index: 1,
            bvid: "BV1xx411c7mD".to_string(),
            aid: 12345,
            cid: 67890,
            owner_name: "UP主".to_string(),
            owner_mid: 4242,
            series_title: "系列".to_string(),
            episode_index: 3,
            episode_title: "第三集".to_string(),
            collection_title: "合集".to_string(),
            index: 7,
            quality: "1080P60".to_string(),
            codec: "AVC".to_string(),
            date: "2026-09-26".to_string(),
            publish_date: "2026-01-02".to_string(),
        }
    }

    fn render_str(template: &str) -> String {
        render(template, &ctx(), "mp4").to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn plain_template_gets_extension() {
        assert_eq!(render_str("{title}"), "标题.mp4");
    }

    #[test]
    fn explicit_ext_is_not_duplicated() {
        assert_eq!(render_str("{title}.{ext}"), "标题.mp4");
    }

    #[test]
    fn slash_makes_directories() {
        assert_eq!(render_str("{title}/{bvid}"), "标题/BV1xx411c7mD.mp4");
    }

    #[test]
    fn part_preset_matches_panel_offer() {
        assert_eq!(
            render_str("{title}/P{part_index} - {part_title}.{ext}"),
            "标题/P1 - 分P标题.mp4"
        );
    }

    #[test]
    fn all_documented_variables_render() {
        // 面板里列出的每个变量都必须真的能替换——这是这个模块存在的理由
        for (token, _) in VARIABLES {
            let rendered = render_str(&format!("x{{{token}}}x"));
            assert_ne!(
                rendered, "x{token}x.mp4",
                "变量 {{{token}}} 在 panels 里列出了，但渲染时没有实现"
            );
        }
    }

    #[test]
    fn unknown_token_stays_visible() {
        assert_eq!(render_str("{titel}"), "{titel}.mp4");
    }

    #[test]
    fn empty_values_disappear_without_leftovers() {
        let mut c = ctx();
        c.episode_title = String::new();
        c.owner_mid = 0;
        let rendered = render("{title}_{episode_title}_{owner_mid}", &c, "mp4")
            .to_string_lossy()
            .to_string();
        // 只留下必要的前后下划线以外的空标记不该出现
        assert!(!rendered.contains('{'), "空变量不该留下字面标记: {rendered}");
        assert!(rendered.starts_with("标题_"));
    }

    #[test]
    fn illegal_characters_are_replaced_per_segment() {
        let mut c = ctx();
        c.title = "a:b*c?d".to_string();
        let rendered = render("{title}/{bvid}", &c, "mp4").to_string_lossy().replace('\\', "/");
        assert_eq!(rendered, "a_b_c_d/BV1xx411c7mD.mp4");
    }

    #[test]
    fn path_traversal_is_dropped() {
        assert_eq!(render_str("../{title}"), "标题.mp4");
        assert_eq!(render_str("{title}/../../etc"), "标题/etc.mp4");
    }

    #[test]
    fn audio_naming_drops_part_placeholders() {
        // 音频没有分集：模板里的分集段落会空掉，文件名不该变成 "P - .m4a"
        let ctx = NamingContext {
            title: "明朝那些事儿149".to_string(),
            ..NamingContext::default()
        };
        assert_eq!(
            render("{title}/P{part_index} - {part_title}.{ext}", &ctx, "m4a"),
            PathBuf::from("明朝那些事儿149.m4a")
        );
        // 正常视频不受影响：分集段落有内容就留着
        let video = NamingContext {
            title: "标题".to_string(),
            part_index: 2,
            part_title: "第二集".to_string(),
            ..NamingContext::default()
        };
        assert_eq!(
            render("{title}/P{part_index} - {part_title}.{ext}", &video, "mp4"),
            PathBuf::from("标题/P2 - 第二集.mp4")
        );
    }

    #[test]
    fn render_dir_drops_extension() {
        let ctx = NamingContext {
            title: "两天在读".to_string(),
            index: 7,
            ..NamingContext::default()
        };
        // 用户模板常以 {ext} 结尾：当目录名时那个点要消失
        assert_eq!(
            render_dir("{title}/P{index} - {part_title}.{ext}", &ctx),
            PathBuf::from("两天在读/P7 -")
        );
        assert_eq!(render_dir("{title}", &ctx), PathBuf::from("两天在读"));
        // 只依赖分集的段落（图文没有分集）整段丢掉，不生成 "P - "
        assert_eq!(
            render_dir("{title}/P{part_index} - {part_title}.{ext}", &ctx),
            PathBuf::from("两天在读")
        );
        // 变量全空时也不能产出空目录名或带尾巴点的名字
        let fallback = render_dir("{bvid}", &NamingContext::default());
        assert!(!fallback.as_os_str().is_empty());
        assert!(!fallback.to_string_lossy().ends_with('.'));
    }

    #[test]
    fn empty_template_falls_back() {
        assert_eq!(render_str(""), "video.mp4");
    }
}
