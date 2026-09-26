//! B 站业务接口与数据结构。

use crate::client::BiliClient;
use crate::error::Result;
use serde::Deserialize;

const API_NAV: &str = "https://api.bilibili.com/x/web-interface/nav";
const API_VIEW: &str = "https://api.bilibili.com/x/web-interface/view";
const API_PLAYURL: &str = "https://api.bilibili.com/x/player/wbi/playurl";

/// 所有 web 接口的统一外壳：`{ code, message, data }`。
#[derive(Debug, Deserialize)]
pub struct ApiEnvelope<T> {
    pub code: i32,
    #[serde(default)]
    pub message: String,
    /// 失败时可能为 null 或缺失；`Option` 字段 serde 天然按可选处理
    pub data: Option<T>,
}

/// 登录态与 wbi 密钥来源。
#[derive(Debug, Clone, Deserialize)]
pub struct NavData {
    #[serde(default, rename = "isLogin")]
    pub is_login: bool,
    #[serde(default)]
    pub uname: String,
    #[serde(default)]
    pub mid: u64,
    #[serde(rename = "wbi_img")]
    pub wbi_img: WbiImg,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WbiImg {
    #[serde(rename = "img_url")]
    pub img_url: String,
    #[serde(rename = "sub_url")]
    pub sub_url: String,
}

/// 稿件信息。
#[derive(Debug, Clone, Deserialize)]
pub struct VideoInfo {
    pub bvid: String,
    #[serde(default)]
    pub aid: u64,
    /// 顶层 cid 即 P1 的分 P id
    #[serde(default)]
    pub cid: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub pic: String,
    /// 秒
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub owner: Owner,
    #[serde(default)]
    pub pages: Vec<Page>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Owner {
    #[serde(default)]
    pub mid: u64,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Page {
    pub cid: u64,
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub part: String,
    #[serde(default)]
    pub duration: u64,
}

/// playurl 返回体。
#[derive(Debug, Clone, Deserialize)]
pub struct PlayUrlData {
    #[serde(default)]
    pub quality: u32,
    #[serde(default, rename = "accept_quality")]
    pub accept_quality: Vec<u32>,
    #[serde(default, rename = "accept_description")]
    pub accept_description: Vec<String>,
    #[serde(default)]
    pub dash: Option<DashData>,
    /// 非 DASH 内容（部分老视频/番剧）会走 durl 分片
    #[serde(default)]
    pub durl: Option<Vec<Durl>>,
}

impl PlayUrlData {
    /// 按期望清晰度挑选视频流：优先 `codecs` 为 avc（兼容性最好），
    /// 期望值不可用时自动降级到不高于它的最高清晰度。
    pub fn pick_video(&self, qn: u32, prefer_avc: bool) -> Option<&MediaStream> {
        let dash = self.dash.as_ref()?;
        if dash.video.is_empty() {
            return None;
        }

        let mut candidates: Vec<&MediaStream> = dash.video.iter().filter(|s| s.id <= qn).collect();
        if candidates.is_empty() {
            // 期望清晰度整体高于账号权限，降级到可用的最高档
            candidates = dash.video.iter().collect();
        }
        candidates.sort_by_key(|s| std::cmp::Reverse(s.id));

        if prefer_avc {
            if let Some(avc) = candidates.iter().find(|s| s.codecs.starts_with("avc")) {
                return Some(avc);
            }
        }
        candidates.first().copied()
    }

    pub fn pick_audio(&self, kind: AudioKind) -> Option<&MediaStream> {
        let dash = self.dash.as_ref()?;
        let special = match kind {
            AudioKind::Flac => dash.flac.as_ref().and_then(|f| f.audio.as_ref()),
            AudioKind::Dolby => dash
                .dolby
                .as_ref()
                .and_then(|d| d.audio.as_ref())
                .and_then(|list| list.first()),
            AudioKind::Normal => None,
        };
        special.or_else(|| best_normal_audio(dash))
    }

    /// 实际可下载的最高画质档位。
    pub fn best_quality(&self) -> u32 {
        self.dash
            .as_ref()
            .and_then(|d| d.video.iter().map(|s| s.id).max())
            .unwrap_or(self.quality)
    }
}

/// DASH 流集合。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DashData {
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub video: Vec<MediaStream>,
    #[serde(default)]
    pub audio: Vec<MediaStream>,
    #[serde(default)]
    pub dolby: Option<DolbyData>,
    #[serde(default)]
    pub flac: Option<FlacData>,
}

/// 音视频轨统一结构。
///
/// 注意：接口会同时返回 camelCase 与 snake_case 两套同名字段（如 `baseUrl` 与 `base_url`），
/// 因此这里只声明 camelCase 一套，snake_case 版本交给 serde 忽略，避免同字段重复赋值报错。
#[derive(Debug, Clone, Deserialize)]
pub struct MediaStream {
    /// 清晰度 / 音频规格 id
    pub id: u32,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(default, rename = "backupUrl")]
    pub backup_url: Vec<String>,
    #[serde(default)]
    pub bandwidth: u64,
    #[serde(default)]
    pub codecs: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default, rename = "mimeType")]
    pub mime_type: String,
}

/// 视频流与音频流结构一致，保留别名以便阅读。
pub type DashStream = MediaStream;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DolbyData {
    #[serde(default)]
    pub audio: Option<Vec<MediaStream>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FlacData {
    #[serde(default)]
    pub audio: Option<MediaStream>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Durl {
    #[serde(default)]
    pub order: u32,
    #[serde(default)]
    pub length: u64,
    #[serde(rename = "url")]
    pub url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioKind {
    Normal,
    Dolby,
    Flac,
}

impl AudioKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "dolby" => Some(Self::Dolby),
            "flac" | "hires" => Some(Self::Flac),
            _ => None,
        }
    }
}

/// 音频档位约定：30216=64k / 30232=132k / 30280=192k 是普通音轨；
/// 30250=杜比全景声、30251=Hi-Res 无损是需要单独指定的特殊音轨，按码率排序时不能混入。
fn best_normal_audio(dash: &DashData) -> Option<&MediaStream> {
    const SPECIAL_AUDIO_IDS: [u32; 2] = [30250, 30251];
    dash.audio
        .iter()
        .filter(|s| !SPECIAL_AUDIO_IDS.contains(&s.id))
        .max_by_key(|s| s.bandwidth)
        .or_else(|| dash.audio.first())
}

/// 清晰度 id → 可读名称。
pub fn quality_name(qn: u32) -> &'static str {
    match qn {
        6 => "240P",
        16 => "360P",
        32 => "480P",
        64 => "720P",
        74 => "720P60",
        80 => "1080P",
        100 => "智能修复",
        112 => "1080P+",
        116 => "1080P60",
        120 => "4K",
        125 => "HDR",
        126 => "杜比视界",
        127 => "8K",
        _ => "未知",
    }
}

impl BiliClient {
    /// 未登录时该接口返回 code=-101，但 data 中仍含 wbi 密钥，因此用宽松模式解析。
    pub async fn nav(&self) -> Result<NavData> {
        self.fetch_json_lenient(API_NAV).await
    }

    pub async fn video_info(&self, bvid: &str) -> Result<VideoInfo> {
        self.fetch_json(&format!("{API_VIEW}?bvid={bvid}")).await
    }

    pub async fn video_info_by_aid(&self, aid: u64) -> Result<VideoInfo> {
        self.fetch_json(&format!("{API_VIEW}?aid={aid}")).await
    }

    /// 取播放地址。`fnval=4048` 一次性索取 DASH + 4K + HDR + 杜比 + 8K。
    pub async fn playurl(&self, bvid: &str, cid: u64, qn: u32) -> Result<PlayUrlData> {
        let url = self.playurl_signed_url(bvid, cid, qn).await?;
        self.fetch_json(&url).await
    }

    /// 单独产出签名后的地址，便于排查请求本身的问题。
    pub async fn playurl_signed_url(&self, bvid: &str, cid: u64, qn: u32) -> Result<String> {
        self.signed_url(
            API_PLAYURL,
            vec![
                ("bvid", bvid.to_string()),
                ("cid", cid.to_string()),
                ("qn", qn.to_string()),
                ("fnver", "0".to_string()),
                ("fnval", "4048".to_string()),
                ("fourk", "1".to_string()),
                ("platform", "pc".to_string()),
            ],
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(id: u32, codecs: &str) -> MediaStream {
        MediaStream {
            id,
            base_url: format!("https://example.com/{id}"),
            backup_url: vec![],
            bandwidth: id as u64 * 1000,
            codecs: codecs.to_string(),
            width: 0,
            height: 0,
            mime_type: String::new(),
        }
    }

    fn play(video: Vec<MediaStream>, audio: Vec<MediaStream>) -> PlayUrlData {
        PlayUrlData {
            quality: 0,
            accept_quality: vec![],
            accept_description: vec![],
            dash: Some(DashData {
                duration: 0,
                video,
                audio,
                dolby: None,
                flac: None,
            }),
            durl: None,
        }
    }

    #[test]
    fn picks_requested_quality_and_prefers_avc() {
        let p = play(
            vec![
                stream(80, "avc1.640028"),
                stream(80, "hev1.1.6"),
                stream(64, "avc1.64001f"),
            ],
            vec![],
        );
        let picked = p.pick_video(80, true).unwrap();
        assert_eq!(picked.id, 80);
        assert!(picked.codecs.starts_with("avc"));
    }

    #[test]
    fn downgrades_when_requested_quality_unavailable() {
        let p = play(vec![stream(32, "avc1"), stream(64, "avc1")], vec![]);
        assert_eq!(
            p.pick_video(120, true).unwrap().id,
            64,
            "应降级到可用的最高档"
        );
        assert_eq!(p.best_quality(), 64);
    }

    #[test]
    fn picks_highest_bandwidth_normal_audio() {
        let p = play(
            vec![],
            vec![
                stream(30216, "mp4a"),
                stream(30280, "mp4a"),
                stream(30251, "flac"),
            ],
        );
        let picked = p.pick_audio(AudioKind::Normal).unwrap();
        assert_eq!(picked.id, 30280, "应排除 Hi-Res 轨并取普通音频最高码率");
    }

    #[test]
    fn parses_audio_kind_names() {
        assert_eq!(AudioKind::parse("FLAC"), Some(AudioKind::Flac));
        assert_eq!(AudioKind::parse("dolby"), Some(AudioKind::Dolby));
        assert_eq!(AudioKind::parse("normal"), Some(AudioKind::Normal));
        assert_eq!(AudioKind::parse("unknown"), None);
    }
}
