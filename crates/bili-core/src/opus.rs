//! 图文（opus/动态）解析。
//!
//! 列表接口 `x/polymer/web-dynamic/v1/opus/feed/space` 只给摘要与封面；
//! 一条图文的**完整**内容（标题、话题、正文、原图）只在 opus 页面的
//! `window.__INITIAL_STATE__` 里，详情接口要风控签名（实测 -352）。
//!
//! 所以下载时抓一次页面、把内嵌状态解出来。状态里的 `modules` 是一个数组，
//! 每项只填自己那类字段，靠 `module_type` 区分。

use crate::error::{BiliError, Result};
use serde::Deserialize;

/// 一条图文（列表用）：摘要 + 封面 + 游标。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpusItem {
    #[serde(default, deserialize_with = "id_str_or_num")]
    pub opus_id: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub content: String,
    #[serde(default)]
    pub jump_url: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub pub_time: String,
    #[serde(default)]
    pub cover: Option<OpusImage>,
}

/// 图文列表分页：`offset` 是下一页游标，`has_more` 判断到底。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpusFeedPage {
    #[serde(default)]
    pub items: Vec<OpusItem>,
    #[serde(default, deserialize_with = "id_str_or_num")]
    pub offset: String,
    #[serde(default)]
    pub has_more: bool,
}

/// 下载用的一条图文：标题、话题、正文、原图。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpusPost {
    pub title: String,
    /// 作者名（页面状态里的 module_author.name）
    pub author: String,
    pub topic: String,
    pub text: String,
    pub images: Vec<OpusImage>,
    pub pub_time: String,
    pub like: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct OpusImage {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

/// 从 opus 页面 HTML 里解出完整图文。
pub fn parse_page(html: &str) -> Result<OpusPost> {
    let state = initial_state(html)?;
    let modules = modules_from_state(&state);

    let mut post = OpusPost::default();
    for (kind, module) in &modules {
        // 类型名两种写法都见过：数组里带 "MODULE_TYPE_TITLE"，对象里是键名 "module_title"
        match kind.to_uppercase().as_str() {
            k if k.contains("TITLE") => {
                post.title = text_of(module.pointer("/module_title/text"));
            }
            k if k.contains("TOPIC") => {
                post.topic = text_of(module.pointer("/module_topic/name"));
            }
            k if k.contains("AUTHOR") => {
                post.pub_time = text_of(module.pointer("/module_author/pub_time"));
                post.author = text_of(module.pointer("/module_author/name"));
            }
            // 图片集形式的图文：图片挂在置顶模块的 album 里，正文段落是纯文本
            k if k.contains("TOP") => {
                if let Some(pics) = module
                    .pointer("/module_top/display/album/pics")
                    .and_then(|v| v.as_array())
                {
                    for pic in pics {
                        if let Ok(image) = serde_json::from_value::<OpusImage>(pic.clone()) {
                            if !image.url.is_empty() && !post.images.contains(&image) {
                                post.images.push(image);
                            }
                        }
                    }
                }
            }
            k if k.contains("STAT") => {
                post.like = module
                    .pointer("/module_stat/like/count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
            }
            k if k.contains("CONTENT") => {
                let paragraphs = module
                    .pointer("/module_content/paragraphs")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                for para in &paragraphs {
                    match para.get("para_type").and_then(|v| v.as_u64()) {
                        // 1 = 文字；把 word 节点拼起来，段落之间空行
                        Some(1) => {
                            let words: String = para
                                .pointer("/text/nodes")
                                .and_then(|v| v.as_array())
                                .map(|nodes| {
                                    nodes
                                        .iter()
                                        .filter_map(|n| n.pointer("/word/words").and_then(|v| v.as_str()))
                                        .collect::<Vec<_>>()
                                        .join("")
                                })
                                .unwrap_or_default();
                            if !words.trim().is_empty() {
                                if !post.text.is_empty() {
                                    post.text.push_str("\n\n");
                                }
                                post.text.push_str(words.trim_end());
                            }
                        }
                        // 2 = 图片
                        Some(2) => {
                            if let Some(pics) = para.pointer("/pic/pics").and_then(|v| v.as_array()) {
                                for pic in pics {
                                    if let Ok(image) = serde_json::from_value::<OpusImage>(pic.clone())
                                    {
                                        if !image.url.is_empty() {
                                            post.images.push(image);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    if post.text.is_empty() && post.images.is_empty() {
        return Err(BiliError::Decode(
            "这个图文没有解析出正文或图片（可能是转发或话题页）".into(),
        ));
    }
    Ok(post)
}

/// 取出 `window.__INITIAL_STATE__ = {...};` 里的对象（按花括号配对，忽略字符串里的括号）。
fn initial_state(html: &str) -> Result<serde_json::Value> {
    const MARKER: &str = "__INITIAL_STATE__";
    let start = html
        .find(MARKER)
        .ok_or_else(|| BiliError::Decode("页面里没有 __INITIAL_STATE__".into()))?;
    let bytes = html.as_bytes();
    let mut i = start + MARKER.len();
    while i < bytes.len() && bytes[i] != b'{' {
        i += 1;
    }
    if i >= bytes.len() {
        return Err(BiliError::Decode("页面状态不是对象".into()));
    }

    let begin = i;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        } else if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                let raw = &html[begin..=i];
                return serde_json::from_str(raw)
                    .map_err(|e| BiliError::Decode(format!("页面状态解析失败: {e}")));
            }
        }
        i += 1;
    }
    Err(BiliError::Decode("页面状态没有闭合".into()))
}

/// 从页面状态里取出各模块，返回 (类型名, 模块) 列表。
///
/// `detail` 有时是对象（图文页）、有时是数组（动态页）；
/// `modules` 有时是数组（每项带 module_type）、有时是按类型名命名的对象。
fn modules_from_state(state: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
    let detail = state.get("detail").cloned().unwrap_or(serde_json::Value::Null);
    let candidates = match detail {
        serde_json::Value::Array(items) => items,
        other => vec![other],
    };
    let mut out = Vec::new();
    for candidate in candidates {
        let Some(modules) = candidate.get("modules") else {
            continue;
        };
        match modules {
            serde_json::Value::Array(items) => {
                for item in items {
                    let kind = item
                        .get("module_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    out.push((kind, item.clone()));
                }
            }
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    out.push((key.clone(), value.clone()));
                }
            }
            _ => {}
        }
    }
    out
}

fn text_of(value: Option<&serde_json::Value>) -> String {
    value.and_then(|v| v.as_str()).unwrap_or_default().to_string()
}

fn id_str_or_num<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::String(s)) => s,
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    })
}

fn text_or_empty<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::String(s)) => s,
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 结构照真页面裁：modules 是数组，各自只填自己那类字段。
    const PAGE: &str = r##"<!DOCTYPE html><html><head><script>
window.__INITIAL_STATE__={"detail":{"basic":{"title":"t"},"modules":[
{"module_type":"MODULE_TYPE_TITLE","module_title":{"text":"#馆长荐书#"}},
{"module_type":"MODULE_TYPE_TOPIC","module_topic":{"name":"今天推荐的书"}},
{"module_type":"MODULE_TYPE_AUTHOR","module_author":{"pub_time":"09-14 20:31"}},
{"module_type":"MODULE_TYPE_STAT","module_stat":{"like":{"count":1639}}},
{"module_type":"MODULE_TYPE_CONTENT","module_content":{"paragraphs":[
 {"para_type":1,"text":{"nodes":[{"type":"TEXT_NODE_TYPE_WORD","word":{"words":"第一段"}},{"word":{"words":"，续写"}}]}},
 {"para_type":2,"pic":{"pics":[{"url":"http://i0.hdslb.com/a.jpg","width":1309,"height":1747}]}},
 {"para_type":2,"pic":{"pics":[{"url":"http://i0.hdslb.com/b.jpg","width":800,"height":600}]}}
]}}]}};</script></head></html>"##;

    #[test]
    fn parses_title_topic_text_and_images() {
        let post = parse_page(PAGE).expect("解析成功");
        assert_eq!(post.title, "#馆长荐书#");
        assert_eq!(post.topic, "今天推荐的书");
        assert_eq!(post.text, "第一段，续写", "word 节点要按顺序拼起来");
        assert_eq!(post.images.len(), 2);
        assert_eq!(post.images[0].url, "http://i0.hdslb.com/a.jpg");
        assert_eq!(post.images[0].height, 1747);
        assert_eq!(post.pub_time, "09-14 20:31");
        assert_eq!(post.like, 1639);
    }

    #[test]
    fn braces_or_quotes_inside_strings_do_not_break_scan() {
        let post = parse_page(&PAGE.replace("第一段", "带 } 和 { 还有 \\\" 引号")).expect("解析成功");
        assert!(post.text.starts_with("带 } 和 {"));
        assert_eq!(post.images.len(), 2, "后面的图片仍然要解出来");
    }

    /// 图片集形式：图片在 module_top.display.album.pics，正文段落里没有图。
    #[test]
    fn parses_album_style_post_images() {
        const PAGE: &str = r##"<html><script>
window.__INITIAL_STATE__={"detail":{"modules":[
{"module_type":"MODULE_TYPE_TOP","module_top":{"display":{"type":1,"album":{"pics":[
 {"url":"http://i0.hdslb.com/bfs/new_dyn/a.jpg","width":1440,"height":1080},
 {"url":"http://i0.hdslb.com/bfs/new_dyn/b.jpg","width":1440,"height":1080},
 {"url":"http://i0.hdslb.com/bfs/new_dyn/c.jpg","width":1440,"height":1080}]}}}},
{"module_type":"MODULE_TYPE_CONTENT","module_content":{"paragraphs":[
 {"para_type":1,"text":{"nodes":[{"word":{"words":"只有文字"}}]}}]}}]}};</script></html>"##;
        let post = parse_page(PAGE).expect("解析成功");
        assert_eq!(post.text, "只有文字");
        assert_eq!(post.images.len(), 3, "图片集里的三张图要都取到");
        assert_eq!(post.images[0].url, "http://i0.hdslb.com/bfs/new_dyn/a.jpg");
    }

    #[test]
    fn rejects_page_without_content() {
        let empty = PAGE.replace(r#""para_type":1"#, r#""para_type":9"#).replace(r#""para_type":2"#, r#""para_type":9"#);
        assert!(parse_page(&empty).is_err());
        assert!(parse_page("<html>没有状态</html>").is_err());
    }
}
