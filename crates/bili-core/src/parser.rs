//! 输入解析：从 BV 号 / av 号 / 各种链接中提取可下载目标。

use crate::error::{BiliError, Result};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Bvid(String),
    Aid(u64),
}

fn bv_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"BV[0-9A-Za-z]{10}").expect("BV 正则合法"))
}

fn av_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?:^|[^0-9A-Za-z])av(\d{1,12})").expect("av 正则合法"))
}

/// 解析用户输入：支持裸 BV 号、av 号，以及包含它们的任意链接文本。
pub fn parse_target(input: &str) -> Result<Target> {
    let s = input.trim();
    if s.is_empty() {
        return Err(BiliError::InvalidInput("输入为空".into()));
    }

    if let Some(m) = bv_re().find(s) {
        return Ok(Target::Bvid(m.as_str().to_string()));
    }

    if let Some(caps) = av_re().captures(s) {
        if let Some(num) = caps.get(1) {
            let aid = num
                .as_str()
                .parse::<u64>()
                .map_err(|_| BiliError::InvalidInput(format!("av 号解析失败: {s}")))?;
            return Ok(Target::Aid(aid));
        }
    }

    Err(BiliError::InvalidInput(format!(
        "没找到 BV 号或 av 号，请检查输入: {s}"
    )))
}

/// b23.tv / 带参数的分享短链，需要先跟随重定向。
pub fn is_short_link(input: &str) -> bool {
    input.contains("b23.tv")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_bvid() {
        assert_eq!(
            parse_target("BV1xx411c7mD").unwrap(),
            Target::Bvid("BV1xx411c7mD".into())
        );
    }

    #[test]
    fn parses_bvid_from_url_with_query() {
        let url = "https://www.bilibili.com/video/BV1xx411c7mD/?spm_id_from=333.999&vd_source=abc";
        assert_eq!(
            parse_target(url).unwrap(),
            Target::Bvid("BV1xx411c7mD".into())
        );
    }

    #[test]
    fn parses_aid() {
        assert_eq!(parse_target("av170001").unwrap(), Target::Aid(170001));
        assert_eq!(
            parse_target("https://www.bilibili.com/video/av170001").unwrap(),
            Target::Aid(170001)
        );
    }

    #[test]
    fn rejects_unrelated_text() {
        assert!(parse_target("https://www.bilibili.com/bangumi/play/ss12345").is_err());
        assert!(parse_target("随便一段文字").is_err());
    }

    #[test]
    fn detects_short_link() {
        assert!(is_short_link("https://b23.tv/abcdefg"));
        assert!(!is_short_link(
            "https://www.bilibili.com/video/BV1xx411c7mD"
        ));
    }
}
