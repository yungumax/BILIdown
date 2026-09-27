//! 输入解析：从 BV 号 / av 号 / 各种链接中提取可下载目标。
//!
//! 支持的来源：
//! - 普通视频：BV 号、av 号、`/video/` 链接、b23.tv 短链（先展开再识别）
//! - 收藏夹：`space.bilibili.com/{mid}/favlist?fid={fid}`
//! - 合集：`space.bilibili.com/{mid}/lists/{sid}` 或 `.../channel/collectiondetail?sid={sid}`
//! - 系列：`space.bilibili.com/{mid}/lists/{sid}?type=series` 或 `.../channel/seriesdetail?sid={sid}`
//! - 图文：`space.bilibili.com/{mid}/upload/opus`（一个 UP 的图文列表）
//! - 音频：`space.bilibili.com/{mid}/upload/audio`（一个 UP 的音频投稿）
//! - UP 空间：`space.bilibili.com/{mid}`
//! - 番剧：`bilibili.com/bangumi/play/ss{nid}` / `.../ep{ep_id}`
//! - 课程：`bilibili.com/cheese/play/ss{nid}`（单集链接需先转成课程页）

use crate::error::{BiliError, Result};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Bvid(String),
    Aid(u64),
    /// 收藏夹：media_id（fid）
    FavList(u64),
    /// 合集：mid + season_id
    Collection {
        mid: u64,
        sid: u64,
    },
    /// 系列：mid + series_id。
    ///
    /// 链接长得跟合集一样（`/lists/{id}`），但数据是另一套接口给的：
    /// 两边的 id 空间重叠，把系列 id 当合集查会命中**别人的**合集
    /// （返回别的内容而且不报错），所以必须靠 `?type=series` 分开走。
    Series {
        mid: u64,
        sid: u64,
    },
    /// UP 空间：mid
    Space(u64),
    /// 图文（opus）列表：mid
    OpusList(u64),
    /// 音频列表：mid
    AudioList(u64),
    /// 番剧：season_id 与 ep_id 至少一个存在
    Bangumi {
        season_id: Option<u64>,
        ep_id: Option<u64>,
    },
    /// 课程：season_id
    Cheese(u64),
}

/// 按识别优先级排列：裸 BV/av 最先，裸空间 mid 最后（避免抢在 favlist/合集之前命中）。
fn patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            Regex::new(r"BV[0-9A-Za-z]{10}").expect("合法"),
            Regex::new(r"(?:^|[^0-9A-Za-z])av(\d{1,12})").expect("合法"),
            Regex::new(r"bilibili\.com/(\d+)/favlist").expect("合法"),
            Regex::new(r"space\.bilibili\.com/(\d+)/upload/opus").expect("合法"),
            Regex::new(r"space\.bilibili\.com/(\d+)/upload/audio").expect("合法"),
            Regex::new(r"space\.bilibili\.com/(\d+)/lists/(\d+)").expect("合法"),
            Regex::new(r"space\.bilibili\.com/(\d+)/[a-z/]*collectiondetail[^ ]*?[?&]sid=(\d+)")
                .expect("合法"),
            Regex::new(r"space\.bilibili\.com/(\d+)/[a-z/]*seriesdetail[^ ]*?[?&]sid=(\d+)")
                .expect("合法"),
            Regex::new(r"bangumi/play/(ss|ep)(\d+)").expect("合法"),
            Regex::new(r"cheese/play/(ss|ep)(\d+)").expect("合法"),
            Regex::new(r"space\.bilibili\.com/(\d+)").expect("合法"),
        ]
    })
}

fn fid_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[?&]fid=(\d+)").expect("合法"))
}

/// 系列链接与合集链接同形（`/lists/{id}`），靠查询串里的 `type=series` 区分。
fn series_query() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[?&]type=series\b").expect("合法"))
}

/// 解析用户输入：支持裸 BV/av 号以及包含它们的任意链接文本。
pub fn parse_target(input: &str) -> Result<Target> {
    let s = input.trim();
    if s.is_empty() {
        return Err(BiliError::InvalidInput("输入为空".into()));
    }

    let patterns = patterns();

    if let Some(m) = patterns[0].find(s) {
        return Ok(Target::Bvid(m.as_str().to_string()));
    }

    if let Some(caps) = patterns[1].captures(s) {
        if let Some(num) = caps.get(1) {
            let aid = num
                .as_str()
                .parse::<u64>()
                .map_err(|_| BiliError::InvalidInput(format!("av 号解析失败: {s}")))?;
            return Ok(Target::Aid(aid));
        }
    }

    // 收藏夹：fid 从查询串提取
    if patterns[2].is_match(s) {
        if let Some(caps) = fid_pattern().captures(s) {
            if let Some(num) = caps.get(1) {
                let fid = num.as_str().parse::<u64>().unwrap_or(0);
                if fid > 0 {
                    return Ok(Target::FavList(fid));
                }
            }
        }
        return Err(BiliError::InvalidInput(
            "收藏夹链接缺少 fid 参数，请在收藏夹页面复制完整链接".into(),
        ));
    }

    // 图文列表：/upload/opus
    if let Some(caps) = patterns[3].captures(s) {
        if let Some(mid) = caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok()) {
            return Ok(Target::OpusList(mid));
        }
    }

    // 音频列表：/upload/audio
    if let Some(caps) = patterns[4].captures(s) {
        if let Some(mid) = caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok()) {
            return Ok(Target::AudioList(mid));
        }
    }

    // 合集 / 系列：新版 /lists/{sid}，两者同形，看 ?type=
    if let Some(caps) = patterns[5].captures(s) {
        if let (Some(mid), Some(sid)) = (
            caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok()),
            caps.get(2).and_then(|m| m.as_str().parse::<u64>().ok()),
        ) {
            return Ok(if series_query().is_match(s) {
                Target::Series { mid, sid }
            } else {
                Target::Collection { mid, sid }
            });
        }
    }

    // 合集：旧版 collectiondetail?sid=
    if let Some(caps) = patterns[6].captures(s) {
        if let (Some(mid), Some(sid)) = (
            caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok()),
            caps.get(2).and_then(|m| m.as_str().parse::<u64>().ok()),
        ) {
            return Ok(Target::Collection { mid, sid });
        }
    }

    // 系列：旧版 seriesdetail?sid=
    if let Some(caps) = patterns[7].captures(s) {
        if let (Some(mid), Some(sid)) = (
            caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok()),
            caps.get(2).and_then(|m| m.as_str().parse::<u64>().ok()),
        ) {
            return Ok(Target::Series { mid, sid });
        }
    }

    // 番剧
    if let Some(caps) = patterns[8].captures(s) {
        let id = caps.get(2).and_then(|m| m.as_str().parse::<u64>().ok());
        match (caps.get(1).map(|m| m.as_str()), id) {
            (Some("ss"), Some(season_id)) => {
                return Ok(Target::Bangumi {
                    season_id: Some(season_id),
                    ep_id: None,
                })
            }
            (Some("ep"), Some(ep_id)) => {
                return Ok(Target::Bangumi {
                    season_id: None,
                    ep_id: Some(ep_id),
                })
            }
            _ => {}
        }
    }

    // 课程
    if let Some(caps) = patterns[9].captures(s) {
        let id = caps.get(2).and_then(|m| m.as_str().parse::<u64>().ok());
        if let (Some("ss"), Some(season_id)) = (caps.get(1).map(|m| m.as_str()), id) {
            return Ok(Target::Cheese(season_id));
        }
        return Err(BiliError::InvalidInput(
            "课程单集链接暂不支持，请使用课程主页链接（/cheese/play/ss…）".into(),
        ));
    }

    // UP 空间（裸 mid）
    if let Some(caps) = patterns[10].captures(s) {
        if let Some(mid) = caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok()) {
            return Ok(Target::Space(mid));
        }
    }

    Err(BiliError::InvalidInput(format!(
        "没找到可识别的目标，请检查输入: {s}"
    )))
}

/// b23.tv 短链需要先跟随重定向。
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
        let url = "https://www.bilibili.com/video/BV1xx411c7mD/?spm_id_from=333.999";
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
    fn parses_favlist_url() {
        let url = "https://space.bilibili.com/123456/favlist?fid=987654&ftype=collect";
        assert_eq!(parse_target(url).unwrap(), Target::FavList(987654));
    }

    #[test]
    fn favlist_without_fid_is_rejected() {
        let url = "https://space.bilibili.com/123456/favlist";
        assert!(parse_target(url).is_err());
    }

    #[test]
    fn parses_opus_list_url() {
        assert_eq!(
            parse_target("https://space.bilibili.com/486287787/upload/opus").unwrap(),
            Target::OpusList(486287787)
        );
        // 带查询串也要认（图文的「图文」tab 就是这条）
        assert_eq!(
            parse_target("space.bilibili.com/486287787/upload/opus?tid=0&page=1").unwrap(),
            Target::OpusList(486287787)
        );
    }

    #[test]
    fn parses_audio_list_url() {
        assert_eq!(
            parse_target("https://space.bilibili.com/649910/upload/audio").unwrap(),
            Target::AudioList(649910)
        );
        assert_eq!(
            parse_target("space.bilibili.com/649910/upload/audio?tid=0").unwrap(),
            Target::AudioList(649910)
        );
    }

    #[test]
    fn parses_collection_lists_url() {
        let url = "https://space.bilibili.com/946974/lists/1764318?type=season";
        assert_eq!(
            parse_target(url).unwrap(),
            Target::Collection {
                mid: 946974,
                sid: 1764318
            }
        );
    }

    /// 系列和合集同形（/lists/{id}），必须按 ?type= 分开：
    /// 拿系列 id 问合集接口会命中别人的合集且不报错，这是真实踩过的坑。
    #[test]
    fn parses_series_lists_url_by_type_query() {
        assert_eq!(
            parse_target("https://space.bilibili.com/486287787/lists/90946?type=series").unwrap(),
            Target::Series {
                mid: 486287787,
                sid: 90946
            }
        );
        // 同一个 id：带 type=season 是合集，不带 type 也按合集处理
        assert!(matches!(
            parse_target("https://space.bilibili.com/486287787/lists/90946?type=season").unwrap(),
            Target::Collection { .. }
        ));
        assert!(matches!(
            parse_target("https://space.bilibili.com/486287787/lists/90946").unwrap(),
            Target::Collection { .. }
        ));
    }

    #[test]
    fn parses_series_detail_url() {
        assert_eq!(
            parse_target("https://space.bilibili.com/486287787/channel/seriesdetail?sid=90946")
                .unwrap(),
            Target::Series {
                mid: 486287787,
                sid: 90946
            }
        );
    }

    #[test]
    fn parses_collection_detail_url() {
        let url = "https://space.bilibili.com/946974/channel/collectiondetail?sid=1764318";
        assert_eq!(
            parse_target(url).unwrap(),
            Target::Collection {
                mid: 946974,
                sid: 1764318
            }
        );
    }

    #[test]
    fn parses_space_url() {
        assert_eq!(
            parse_target("https://space.bilibili.com/1858731").unwrap(),
            Target::Space(1858731)
        );
        assert_eq!(
            parse_target("space.bilibili.com/1858731/dynamic").unwrap(),
            Target::Space(1858731)
        );
    }

    #[test]
    fn parses_bangumi_ss_and_ep() {
        assert_eq!(
            parse_target("https://www.bilibili.com/bangumi/play/ss32980").unwrap(),
            Target::Bangumi {
                season_id: Some(32980),
                ep_id: None
            }
        );
        assert_eq!(
            parse_target("https://www.bilibili.com/bangumi/play/ep219026").unwrap(),
            Target::Bangumi {
                season_id: None,
                ep_id: Some(219026)
            }
        );
    }

    #[test]
    fn parses_cheese_ss() {
        assert_eq!(
            parse_target("https://www.bilibili.com/cheese/play/ss1234").unwrap(),
            Target::Cheese(1234)
        );
    }

    #[test]
    fn rejects_unrelated_text() {
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
