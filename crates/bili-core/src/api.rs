//! B 站业务接口与数据结构。

use crate::client::BiliClient;
use crate::error::{BiliError, Result};
use crate::opus::OpusFeedPage;
use serde::{Deserialize, Deserializer};

const API_NAV: &str = "https://api.bilibili.com/x/web-interface/nav";
const API_VIEW: &str = "https://api.bilibili.com/x/web-interface/view";
const API_PLAYURL: &str = "https://api.bilibili.com/x/player/wbi/playurl";
const API_FAV_LIST: &str = "https://api.bilibili.com/x/v3/fav/resource/list";
const API_SEASONS_ARCHIVES: &str =
    "https://api.bilibili.com/x/polymer/web-space/seasons_archives_list";
/// 系列：与合集同形（`/lists/{id}`）却是另一套接口，id 空间还重叠，
/// 拿系列 id 问合集接口会返回别人的合集且不报错，所以必须分开调。
const API_SERIES_ARCHIVES: &str = "https://api.bilibili.com/x/series/archives";
const API_SERIES_META: &str = "https://api.bilibili.com/x/series/series";
const API_SPACE_ARC: &str = "https://api.bilibili.com/x/space/wbi/arc/search";
/// UP 主页的合集/系列列表（用来判断每条投稿属于哪个合集）
const API_SEASONS_SERIES: &str = "https://api.bilibili.com/x/polymer/web-space/home/seasons_series";
/// 图文（opus）列表：按 offset 游标翻页（page 参数无效，实测会重复返回第一页）
const API_OPUS_FEED: &str = "https://api.bilibili.com/x/polymer/web-dynamic/v1/opus/feed/space";
/// 音频投稿列表（必须带 order/platform，否则静默返回空列表）
const API_AUDIO_LIST: &str = "https://api.bilibili.com/audio/music-service-c/web/song/upper";
/// 音频播放地址（一页最多 30 条的接口，这里是单曲）
const API_AUDIO_URL: &str = "https://api.bilibili.com/audio/music-service-c/web/url";
const API_PGC_SEASON: &str = "https://api.bilibili.com/pgc/view/web/season";
const API_PGC_PLAYURL: &str = "https://api.bilibili.com/pgc/player/web/playurl";
const API_PUGV_SEASON: &str = "https://api.bilibili.com/pugv/view/web/season";
const API_PUGV_PLAYURL: &str = "https://api.bilibili.com/pugv/player/web/playurl";

// 该接口在不同登录状态/不同版本下，字段类型会变（缺失、null、字符串、对象都出现过），
// 下面两个适配器把「缺失或 null」统一收敛成安全默认值，避免整份响应解析失败。

/// 缺失或 null 一律读成空集合。
pub(crate) fn vec_or_null<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// 缺失或 null 一律读成 0。
pub(crate) fn u64_or_null<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<u64>::deserialize(deserializer)?.unwrap_or(0))
}

/// 缺失或 null 一律读成空字符串。
pub(crate) fn string_or_null<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// 所有 web 接口的统一外壳：`{ code, message, data }`。
#[derive(Debug, Deserialize)]
pub struct ApiEnvelope<T> {
    pub code: i32,
    #[serde(default)]
    pub message: String,
    /// 失败时可能为 null 或缺失；`Option` 字段 serde 天然按可选处理
    pub data: Option<T>,
}

/// 大会员标签。
///
/// 登录后 `vip_label` 是对象（含 `text` 等），未登录时可能是字符串或缺失，
/// 因此用无标签枚举同时兼容两种形态。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum VipLabel {
    Object {
        #[serde(default)]
        text: String,
        #[serde(default)]
        label_theme: String,
    },
    Text(String),
}

impl VipLabel {
    pub fn text(&self) -> &str {
        match self {
            VipLabel::Object { text, .. } => text,
            VipLabel::Text(text) => text,
        }
    }
}

/// 登录态与 wbi 密钥来源。
#[derive(Debug, Clone, Deserialize)]
pub struct NavData {
    #[serde(default, rename = "isLogin")]
    pub is_login: bool,
    #[serde(default, deserialize_with = "string_or_null")]
    pub uname: String,
    /// 用户头像 URL（i*.hdslb.com，https；未登录时可能为空）。
    #[serde(default, deserialize_with = "string_or_null")]
    pub face: String,
    #[serde(default)]
    pub mid: u64,
    /// 0=非大会员，1=大会员
    #[serde(default, rename = "vipStatus")]
    pub vip_status: u32,
    #[serde(default, rename = "vip_label")]
    pub vip_label: Option<VipLabel>,
    #[serde(rename = "wbi_img")]
    pub wbi_img: WbiImg,
}

impl NavData {
    /// 大会员标签文案，没有则为空串。
    pub fn vip_label_text(&self) -> &str {
        self.vip_label.as_ref().map(VipLabel::text).unwrap_or("")
    }
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
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub desc: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub pic: String,
    /// 秒
    #[serde(default)]
    pub duration: u64,
    /// B 站发布时间的 Unix 秒，用于命名模板的 {publish_date}
    #[serde(default)]
    pub pubdate: u64,
    #[serde(default)]
    pub owner: Owner,
    /// 该视频所属的合集（不在合集里时为 None）
    #[serde(default)]
    pub ugc_season: Option<UgcSeason>,
    #[serde(default, deserialize_with = "vec_or_null")]
    pub pages: Vec<Page>,
}

/// 视频所属的合集。批量解析时用它把"一个视频"展开成"整个合集"：
/// 一条 video_info 就带回合集全部成员的 bvid 与 cid。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UgcSeason {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub mid: u64,
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    #[serde(default, deserialize_with = "vec_or_null")]
    pub sections: Vec<SeasonSection>,
}

/// 合集内的一个分区（通常只有一个，成员都在它的 episodes 里）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeasonSection {
    #[serde(default, deserialize_with = "vec_or_null")]
    pub episodes: Vec<SeasonEpisode>,
}

/// 合集成员条目：bvid 与 cid 都在，cid 正是 playurl 要的分 P id。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeasonEpisode {
    #[serde(default)]
    pub bvid: String,
    #[serde(default)]
    pub cid: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Owner {
    #[serde(default)]
    pub mid: u64,
    #[serde(default, deserialize_with = "string_or_null")]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Page {
    pub cid: u64,
    #[serde(default)]
    pub page: u32,
    #[serde(default, deserialize_with = "string_or_null")]
    pub part: String,
    #[serde(default)]
    pub duration: u64,
}

/// playurl 返回体。
#[derive(Debug, Clone, Deserialize)]
pub struct PlayUrlData {
    #[serde(default)]
    pub quality: u32,
    #[serde(default, rename = "accept_quality", deserialize_with = "vec_or_null")]
    pub accept_quality: Vec<u32>,
    #[serde(
        default,
        rename = "accept_description",
        deserialize_with = "vec_or_null"
    )]
    pub accept_description: Vec<String>,
    #[serde(default)]
    pub dash: Option<DashData>,
    /// 非 DASH 内容（部分老视频/番剧）会走 durl 分片
    #[serde(default)]
    pub durl: Option<Vec<Durl>>,
}

impl PlayUrlData {
    /// 按期望清晰度挑选视频流：先锁定不高于期望值的最高档，再按编码偏好
    /// 在同档内挑选（avc 兼容性最好 / hevc 压缩率最高 / auto 取首条）。
    pub fn pick_video(&self, qn: u32, codec_pref: &str) -> Option<&MediaStream> {
        let dash = self.dash.as_ref()?;
        if dash.video.is_empty() {
            return None;
        }

        let mut candidates: Vec<&MediaStream> = dash.video.iter().filter(|s| s.id <= qn).collect();
        if candidates.is_empty() {
            // 期望清晰度整体高于账号权限，降级到可用的最高档
            candidates = dash.video.iter().collect();
        }

        // 先锁定最高清晰度档，再在同一档内比较编码。
        // 若跨档位去挑 avc，会把更高的 HEVC 档（如杜比视界）白白跳过。
        let best_id = candidates.iter().map(|s| s.id).max()?;
        let same_quality: Vec<&MediaStream> =
            candidates.into_iter().filter(|s| s.id == best_id).collect();

        pick_by_codec(&same_quality, codec_pref)
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

    /// 按优先顺序挑视频流。
    ///
    /// 逐条尝试：命中该档位就取；同档内多个编码时按这一条的编码偏好选。
    /// 全部没命中时按 `fallback` 处理：`fail` 返回 None，否则取可用的最高档
    /// （编码偏好沿用链上第一条明确指定的编码）。
    pub fn pick_video_chain(
        &self,
        chain: &[(u32, String)],
        fallback: &str,
    ) -> Option<&MediaStream> {
        let dash = self.dash.as_ref()?;
        if dash.video.is_empty() {
            return None;
        }

        for (qn, codec) in chain {
            let same_quality: Vec<&MediaStream> =
                dash.video.iter().filter(|s| s.id == *qn).collect();
            if same_quality.is_empty() {
                continue;
            }
            if let Some(picked) = pick_by_codec(&same_quality, codec) {
                return Some(picked);
            }
        }

        if fallback == "fail" {
            return None;
        }
        // 回退：可用的最高档，编码偏好取链上第一条非 auto 的
        let codec = chain
            .iter()
            .map(|(_, codec)| codec.as_str())
            .find(|codec| *codec != "auto")
            .unwrap_or("auto");
        let best_id = dash.video.iter().map(|s| s.id).max()?;
        let same_quality: Vec<&MediaStream> =
            dash.video.iter().filter(|s| s.id == best_id).collect();
        pick_by_codec(&same_quality, codec)
    }

    /// 按优先顺序挑音轨：逐条尝试，都没有时退回普通音轨（普通音轨总在）。
    pub fn pick_audio_chain(&self, chain: &[String]) -> Option<&MediaStream> {
        let dash = self.dash.as_ref()?;
        for kind in chain {
            let special = match kind.as_str() {
                "flac" => dash.flac.as_ref().and_then(|f| f.audio.as_ref()),
                "dolby" => dash
                    .dolby
                    .as_ref()
                    .and_then(|d| d.audio.as_ref())
                    .and_then(|list| list.first()),
                // auto / normal 都是普通音轨，auto 取最好的一条
                _ => None,
            };
            if let Some(stream) = special {
                return Some(stream);
            }
            if kind == "auto" || kind == "normal" {
                if let Some(normal) = best_normal_audio(dash) {
                    return Some(normal);
                }
            }
        }
        best_normal_audio(dash)
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
    #[serde(default, deserialize_with = "vec_or_null")]
    pub video: Vec<MediaStream>,
    #[serde(default, deserialize_with = "vec_or_null")]
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
    #[serde(rename = "baseUrl", deserialize_with = "string_or_null")]
    pub base_url: String,
    #[serde(default, rename = "backupUrl", deserialize_with = "vec_or_null")]
    pub backup_url: Vec<String>,
    #[serde(default)]
    pub bandwidth: u64,
    #[serde(default, deserialize_with = "string_or_null")]
    pub codecs: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default, rename = "mimeType", deserialize_with = "string_or_null")]
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

// ---------- 批量来源的数据结构 ----------

/// 收藏夹分页。`medias` 自带 cid，下载时无需再查稿件信息。
#[derive(Debug, Clone, Deserialize)]
pub struct FavPage {
    #[serde(default)]
    pub info: FavInfo,
    #[serde(default, deserialize_with = "vec_or_null")]
    pub medias: Vec<FavMedia>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FavInfo {
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub upper_name: String,
    #[serde(default)]
    pub media_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FavMedia {
    pub bvid: String,
    /// 实测部分条目的 cid 为 null，容错为 0（下载时按 bvid 补查）
    #[serde(default, deserialize_with = "u64_or_null")]
    pub cid: u64,
    #[serde(default, deserialize_with = "string_or_null")]
    pub cover: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    /// 秒
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub upper: FavUpper,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FavUpper {
    #[serde(default, deserialize_with = "string_or_null")]
    pub name: String,
}

/// 合集分页。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeasonArchivesPage {
    #[serde(default)]
    pub meta: SeasonMeta,
    #[serde(default, rename = "archives", deserialize_with = "vec_or_null")]
    pub archives: Vec<SeasonArchive>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeasonMeta {
    #[serde(default)]
    pub season_id: u64,
    #[serde(default, deserialize_with = "string_or_null")]
    pub name: String,
    #[serde(default)]
    pub total: u32,
    /// 合集所属 UP 的 mid（条目里没有上传者，只能拿这个反查名字）
    #[serde(default)]
    pub mid: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeasonArchive {
    pub bvid: String,
    #[serde(default)]
    pub cid: u64,
    #[serde(default, deserialize_with = "string_or_null")]
    pub pic: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    /// 秒
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub owner: Owner,
}

/// UP 主的合集/系列列表。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeasonsSeriesPage {
    #[serde(default)]
    pub items_lists: SeasonsSeriesItems,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeasonsSeriesItems {
    #[serde(default, rename = "seasons_list", deserialize_with = "vec_or_null")]
    pub seasons: Vec<SeasonSummary>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeasonSummary {
    #[serde(default)]
    pub meta: SeasonMeta,
}

/// 音频投稿分页。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AudioListPage {
    #[serde(default, rename = "data", deserialize_with = "vec_or_null")]
    pub items: Vec<AudioItem>,
    #[serde(default, rename = "totalSize")]
    pub total_size: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AudioItem {
    #[serde(default)]
    pub id: u64,
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    /// 秒
    #[serde(default)]
    pub duration: u64,
    #[serde(default, deserialize_with = "string_or_null")]
    pub uname: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub author: String,
}

/// 音频播放地址。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AudioStream {
    #[serde(default, deserialize_with = "vec_or_null")]
    pub cdns: Vec<String>,
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
}

/// 系列内容分页（`x/series/archives`）：条目字段比合集少，没有 cid 与上传者。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeriesArchivesPage {
    #[serde(default, rename = "archives", deserialize_with = "vec_or_null")]
    pub archives: Vec<SeriesArchive>,
    #[serde(default)]
    pub page: SeriesPageInfo,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeriesPageInfo {
    #[serde(default)]
    pub total: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesArchive {
    pub bvid: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub pic: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    /// 秒
    #[serde(default)]
    pub duration: u64,
}

/// 系列本身的元信息（`x/series/series`）：标题在这里。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeriesMetaPage {
    #[serde(default)]
    pub meta: SeriesMeta,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeriesMeta {
    #[serde(default, deserialize_with = "string_or_null")]
    pub name: String,
    #[serde(default)]
    pub mid: u64,
}

/// UP 空间投稿分页。`vlist` 不含 cid，下载时按 bvid 补查。
#[derive(Debug, Clone, Deserialize)]
pub struct SpaceArcPage {
    #[serde(default)]
    pub page: SpacePageInfo,
    #[serde(default, rename = "list")]
    pub list: Option<SpaceArcList>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SpacePageInfo {
    #[serde(default)]
    pub count: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SpaceArcList {
    #[serde(default, rename = "vlist", deserialize_with = "vec_or_null")]
    pub vlist: Vec<SpaceVideo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpaceVideo {
    pub bvid: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub pic: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    /// "mm:ss" 文本
    #[serde(default, deserialize_with = "string_or_null")]
    pub length: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub author: String,
}

/// 番剧剧集信息。episodes 的 duration 为毫秒。
#[derive(Debug, Clone, Deserialize)]
pub struct PgcSeason {
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    /// 出品方/上传者，列表里作为每集的 UP 主展示
    #[serde(default)]
    pub up_info: UpInfo,
    #[serde(default, deserialize_with = "vec_or_null")]
    pub episodes: Vec<PgcEpisode>,
}

/// 番剧/课程的上传者信息（只需要名字）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpInfo {
    #[serde(default, deserialize_with = "string_or_null")]
    pub uname: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PgcEpisode {
    pub bvid: String,
    #[serde(default)]
    pub cid: u64,
    #[serde(default)]
    pub id: u64,
    #[serde(default, deserialize_with = "string_or_null")]
    pub long_title: String,
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    #[serde(default)]
    pub duration: u64,
}

/// 课程信息。
#[derive(Debug, Clone, Deserialize)]
pub struct PugvSeason {
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    #[serde(default)]
    pub up_info: UpInfo,
    #[serde(default, deserialize_with = "vec_or_null")]
    pub episodes: Vec<PugvEpisode>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PugvEpisode {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub cid: u64,
    #[serde(default, deserialize_with = "string_or_null")]
    pub title: String,
    /// 秒（与番剧的毫秒不同）
    #[serde(default)]
    pub duration: u64,
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
/// 在同一清晰度档内按编码偏好挑一条；偏好编码不存在时取该档第一条。
fn pick_by_codec<'a>(
    same_quality: &[&'a MediaStream],
    codec_pref: &str,
) -> Option<&'a MediaStream> {
    let wanted: Option<&MediaStream> = match codec_pref {
        "avc" => same_quality
            .iter()
            .find(|s| s.codecs.starts_with("avc"))
            .copied(),
        "hevc" => same_quality
            .iter()
            .find(|s| s.codecs.starts_with("hev") || s.codecs.starts_with("hvc"))
            .copied(),
        "av1" => same_quality
            .iter()
            .find(|s| s.codecs.starts_with("av01"))
            .copied(),
        _ => None,
    };
    wanted.or_else(|| same_quality.first().copied())
}

fn best_normal_audio(dash: &DashData) -> Option<&MediaStream> {
    const SPECIAL_AUDIO_IDS: [u32; 2] = [30250, 30251];
    dash.audio
        .iter()
        .filter(|s| !SPECIAL_AUDIO_IDS.contains(&s.id))
        .max_by_key(|s| s.bandwidth)
        .or_else(|| dash.audio.first())
}

/// 用户名片里只关心名字（合集接口只给 mid，不给每条的上传者）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserCardData {
    #[serde(default)]
    pub card: UserCard,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserCard {
    #[serde(default, deserialize_with = "string_or_null")]
    pub name: String,
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

/// 编码名：判断依据是 DASH 流的 codecs 字段（avc1.640033 / hev1.1.6 / av01.0.12M.08）。
pub fn codec_name(codecs: &str) -> &'static str {
    if codecs.starts_with("hev") || codecs.starts_with("hvc") {
        "HEVC"
    } else if codecs.starts_with("av01") {
        "AV1"
    } else if codecs.starts_with("avc") {
        "AVC"
    } else {
        "未知"
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

    // ---------- 批量来源：收藏夹 / 合集 / UP 空间 ----------

    /// 收藏夹内容（一页 20 条）。`media_id` 即链接里的 fid。
    /// 取 UP 主名字；失败返回空字符串（列表里显示成「—」，不影响下载）。
    pub async fn user_name(&self, mid: u64) -> String {
        let url = format!("https://api.bilibili.com/x/web-interface/card?mid={mid}");
        match self.fetch_json::<UserCardData>(&url).await {
            Ok(data) => data.card.name,
            Err(_) => String::new(),
        }
    }

    pub async fn fav_list(&self, media_id: u64, page: u32) -> Result<FavPage> {
        let url = format!(
            "{API_FAV_LIST}?media_id={media_id}&pn={page}&ps=20&order=mtime&type=2&tid=0&platform=web"
        );
        self.fetch_json(&url).await
    }

    /// UP 主合集内容（一页最多 100 条）。
    pub async fn seasons_archives(
        &self,
        mid: u64,
        season_id: u64,
        page: u32,
    ) -> Result<SeasonArchivesPage> {
        let url = format!(
            "{API_SEASONS_ARCHIVES}?mid={mid}&season_id={season_id}&page_num={page}&page_size=100"
        );
        // 翻过末页会返回 data: null，按"这一页没有内容"处理
        Ok(self.fetch_json_opt(&url).await?.unwrap_or_default())
    }

    /// 这条视频属于哪个合集（不属于任何合集时返回 None）。
    ///
    /// 逐条查而不是先枚举 UP 的全部合集：合集列表接口连查十几个就 -352 风控，
    /// 而视频详情本来就随下载逐条请求，节奏天然安全。
    pub async fn collection_of(&self, bvid: &str) -> Option<String> {
        let info = self.video_info(bvid).await.ok()?;
        let season = info.ugc_season?;
        let title = season.title.trim();
        if season.id == 0 || title.is_empty() {
            None
        } else {
            Some(title.to_string())
        }
    }

    /// UP 主的合集列表（一页最多 20 个合集）。
    pub async fn space_collections(&self, mid: u64, page: u32) -> Result<SeasonsSeriesPage> {
        let url = format!("{API_SEASONS_SERIES}?mid={mid}&page_num={page}&page_size=20");
        self.fetch_json(&url).await
    }

    /// UP 主系列内容（一页最多 30 条；条目里没有 cid，下载时按 bvid 补查）。
    pub async fn series_archives(
        &self,
        mid: u64,
        series_id: u64,
        page: u32,
    ) -> Result<SeriesArchivesPage> {
        let url = format!(
            "{API_SERIES_ARCHIVES}?mid={mid}&series_id={series_id}&only_normal=true&sort=desc&pn={page}&ps=30"
        );
        Ok(self.fetch_json_opt(&url).await?.unwrap_or_default())
    }

    /// 系列的名称：`archives` 接口只给条目，不给标题。
    pub async fn series_meta(&self, mid: u64, series_id: u64) -> Result<SeriesMetaPage> {
        let url = format!("{API_SERIES_META}?mid={mid}&series_id={series_id}");
        self.fetch_json(&url).await
    }

    /// UP 主图文（opus）列表：20 条一页，`offset` 为下一页游标。
    pub async fn opus_feed(&self, mid: u64, offset: &str) -> Result<OpusFeedPage> {
        let mut url = format!("{API_OPUS_FEED}?host_mid={mid}&type=all&page=1");
        if !offset.is_empty() {
            url.push_str(&format!("&offset={offset}"));
        }
        self.fetch_json(&url).await
    }

    /// UP 主的音频投稿（一页最多 30 条）。
    ///
    /// 务必带上 `order` 与 `platform`：少了它们接口 code 仍是 0，
    /// 却返回 `data: null`——看起来像"这个 UP 没有音频"。
    pub async fn audio_list(&self, mid: u64, page: u32) -> Result<AudioListPage> {
        let url = format!("{API_AUDIO_LIST}?uid={mid}&pn={page}&ps=30&order=1&platform=web");
        // 这个接口末页之后会返回 data: null（报的总数还常常大于实取条数），
        // 必须按"没有更多"处理，否则翻到底会报格式异常
        Ok(self.fetch_json_opt(&url).await?.unwrap_or_default())
    }

    /// 音频播放地址：拿到可直接下载的 m4a（cdns 首个即推荐线路）。
    pub async fn audio_stream(&self, sid: u64) -> Result<AudioStream> {
        let url = format!("{API_AUDIO_URL}?sid={sid}&privilege=2&quality=2");
        self.fetch_json(&url).await
    }

    /// 单条图文/专栏的页面 HTML。专栏的旧链接会 301 到 opus 页，客户端跟随重定向，
    /// 所以两种形态拿到的是同一份页面状态。
    pub async fn post_page(&self, id: u64, article: bool) -> Result<String> {
        let url = if article {
            format!("https://www.bilibili.com/read/cv{id}")
        } else {
            format!("https://www.bilibili.com/opus/{id}")
        };
        self.fetch_text(&url).await
    }

    /// 图文详情页的 HTML：完整内容（正文与原图）只在这份页面状态里。
    pub async fn opus_page(&self, opus_id: &str) -> Result<String> {
        self.fetch_text(&format!("https://www.bilibili.com/opus/{opus_id}"))
            .await
    }

    /// UP 主投稿列表（一页 30 条，需 wbi 签名）。
    pub async fn space_archives(&self, mid: u64, page: u32) -> Result<SpaceArcPage> {
        let url = self
            .signed_url(
                API_SPACE_ARC,
                vec![
                    ("mid", mid.to_string()),
                    ("pn", page.to_string()),
                    ("ps", "30".to_string()),
                    ("order", "pubdate".to_string()),
                    ("platform", "web".to_string()),
                ],
            )
            .await?;
        self.fetch_json(&url).await
    }

    // ---------- 番剧 ----------

    /// 番剧剧集信息：按 season_id 或 ep_id 查询均可。
    pub async fn pgc_season(
        &self,
        season_id: Option<u64>,
        ep_id: Option<u64>,
    ) -> Result<PgcSeason> {
        let query = match (season_id, ep_id) {
            (Some(id), _) => format!("season_id={id}"),
            (None, Some(ep)) => format!("ep_id={ep}"),
            (None, None) => return Err(BiliError::InvalidInput("缺少番剧 id".into())),
        };
        self.fetch_pgc_json(&format!("{API_PGC_SEASON}?{query}"))
            .await
    }

    /// 番剧播放地址（免费内容可直接取，付费内容需有效登录态）。
    pub async fn pgc_playurl(
        &self,
        bvid: &str,
        cid: u64,
        ep_id: Option<u64>,
        qn: u32,
    ) -> Result<PlayUrlData> {
        let mut params = vec![
            ("cid", cid.to_string()),
            ("qn", qn.to_string()),
            ("fnval", "4048".to_string()),
            ("fourk", "1".to_string()),
            ("platform", "pc".to_string()),
        ];
        if let Some(ep) = ep_id {
            params.push(("ep_id", ep.to_string()));
        }
        if !bvid.is_empty() {
            params.push(("bvid", bvid.to_string()));
        }
        let url = format!("{API_PGC_PLAYURL}?{}", encode_query(&params));
        self.fetch_pgc_json(&url).await
    }

    // ---------- 课程 ----------

    /// 课程信息（含每集时长等元数据）。
    pub async fn cheese_season(&self, season_id: u64) -> Result<PugvSeason> {
        self.fetch_json(&format!("{API_PUGV_SEASON}?season_id={season_id}"))
            .await
    }

    /// 课程播放地址。
    pub async fn cheese_playurl(&self, ep_id: u64, cid: u64, qn: u32) -> Result<PlayUrlData> {
        let url = format!(
            "{API_PUGV_PLAYURL}?ep_id={ep_id}&cid={cid}&qn={qn}&fnval=4048&fourk=1&platform=pc"
        );
        self.fetch_json(&url).await
    }
}

/// 简单查询串编码（本组接口的参数都是数字/固定枚举，无需完整百分号编码）。
fn encode_query(params: &[(&str, String)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 登录后的 nav 响应（字段取自真实响应，仅替换隐私字段）。
    ///
    /// 回归点：登录后 `vip_label` 是对象而非字符串，未登录时缺失——曾经因为把它
    /// 声明成 String 导致整份响应解析失败，登录被误判为未成功。
    #[test]
    fn parses_logged_in_nav_response() {
        let raw = r##"{
            "code": 0, "message": "OK", "ttl": 1,
            "data": {
                "isLogin": true,
                "email_verified": 1,
                "face": "http://i0.hdslb.com/bfs/face/example.gif",
                "face_nft": 0,
                "face_nft_type": 0,
                "level_info": {"current_level": 6, "current_min": 28800, "current_exp": 39630, "next_exp": "--"},
                "mid": 1858731,
                "mobile_verified": 1,
                "money": 0,
                "moral": 70,
                "official": {"role": 0, "title": "", "desc": "", "type": -1},
                "pendant": {"pid": 0, "name": "", "image": "", "expire": 0},
                "uname": "测试用户",
                "vipDueDate": 1760000000000,
                "vipStatus": 1,
                "vipType": 2,
                "vip_pay_type": 0,
                "vip_theme_type": 0,
                "vip_label": {
                    "path": "http://i0.hdslb.com/bfs/vip/label_annual.png",
                    "text": "年度大会员",
                    "label_theme": "annual_vip",
                    "text_color": "#FFFFFF",
                    "bg_style": 1
                },
                "vip_avatar_subscript": 1,
                "vip": {
                    "type": 2, "status": 1, "due_date": 1760000000000,
                    "label": {"path": "", "text": "年度大会员", "label_theme": "annual_vip"},
                    "avatar_subscript": 1, "nickname_color": "#FB7299", "role": 3
                },
                "wallet": {"mid": 1858731, "bcoin_balance": 0, "coupon_balance": 0},
                "has_shop": false,
                "is_jury": false,
                "wbi_img": {
                    "img_url": "https://i0.hdslb.com/bfs/wbi/7cd084941338484aae1ad9425b84077c.png",
                    "sub_url": "https://i0.hdslb.com/bfs/wbi/4932caff0ff746eab6f01bf08b70ac45.png"
                }
            }
        }"##;

        let envelope: ApiEnvelope<NavData> =
            serde_json::from_str(raw).expect("登录后的 nav 响应应能解析");
        let nav = envelope.data.expect("应带 data");

        assert!(nav.is_login);
        assert_eq!(nav.uname, "测试用户");
        assert_eq!(nav.mid, 1858731);
        assert_eq!(nav.vip_status, 1);
        assert_eq!(nav.vip_label_text(), "年度大会员");
        assert_eq!(
            nav.wbi_img.img_url.rsplit('/').next().unwrap(),
            "7cd084941338484aae1ad9425b84077c.png"
        );
    }

    #[test]
    fn parses_logged_out_nav_without_vip_label() {
        let raw = r#"{"code": -101, "message": "账号未登录", "ttl": 1, "data": {
            "isLogin": false,
            "wbi_img": {"img_url": "https://i0.hdslb.com/bfs/wbi/aaa.png", "sub_url": "https://i0.hdslb.com/bfs/wbi/bbb.png"}
        }}"#;
        let envelope: ApiEnvelope<NavData> = serde_json::from_str(raw).unwrap();
        let nav = envelope.data.unwrap();
        assert!(!nav.is_login);
        assert_eq!(nav.vip_label_text(), "");
    }

    #[test]
    fn tolerates_null_fields_in_playurl() {
        let raw = r#"{
            "code": 0, "message": "OK",
            "data": {
                "accept_quality": null,
                "accept_description": null,
                "dash": {
                    "duration": 18,
                    "video": [{
                        "id": 80, "baseUrl": "https://example.com/v.m4s",
                        "backupUrl": null, "bandwidth": 1000,
                        "codecs": null, "mimeType": null, "width": 1920, "height": 1080
                    }],
                    "audio": null
                }
            }
        }"#;
        let envelope: ApiEnvelope<PlayUrlData> =
            serde_json::from_str(raw).expect("字段为 null 时也应能解析");
        let play = envelope.data.unwrap();
        let picked = play.pick_video(80, "avc").expect("应选出视频流");
        assert_eq!(picked.id, 80);
        assert!(picked.backup_url.is_empty());
        assert_eq!(picked.codecs, "");
        assert!(
            play.pick_audio(AudioKind::Normal).is_none(),
            "音频为 null 应视为无音轨"
        );
    }

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

    fn play_with_special(
        video: Vec<MediaStream>,
        audio: Vec<MediaStream>,
        dolby: Option<Vec<MediaStream>>,
        flac: Option<Vec<MediaStream>>,
    ) -> PlayUrlData {
        PlayUrlData {
            quality: 0,
            accept_quality: vec![],
            accept_description: vec![],
            dash: Some(DashData {
                duration: 0,
                video,
                audio,
                dolby: dolby.map(|audio| DolbyData { audio: Some(audio) }),
                // flac 在接口里是单条流，不是列表
                flac: flac.map(|audio| FlacData {
                    audio: audio.into_iter().next(),
                }),
            }),
            durl: None,
        }
    }

    fn pref(qn: u32, codec: &str) -> (u32, String) {
        (qn, codec.to_string())
    }

    #[test]
    fn video_chain_takes_the_first_available_preference() {
        let p = play(vec![stream(80, "avc1"), stream(120, "hev1")], vec![]);
        // 8K 没有 → 落到链上第二条 1080P
        let chain = vec![pref(127, "auto"), pref(80, "auto")];
        assert_eq!(p.pick_video_chain(&chain, "nearest").unwrap().id, 80);

        // 顺序反过来就应拿到 4K，优先顺序是用户排的，不能被"最高档"覆盖
        let chain = vec![pref(120, "auto"), pref(80, "auto")];
        assert_eq!(p.pick_video_chain(&chain, "nearest").unwrap().id, 120);
    }

    #[test]
    fn video_chain_prefers_codec_inside_the_matched_tier() {
        let p = play(
            vec![stream(80, "hev1.1.6"), stream(80, "avc1.640028")],
            vec![],
        );
        let picked = p.pick_video_chain(&[pref(80, "avc")], "nearest").unwrap();
        assert_eq!(picked.id, 80);
        assert!(picked.codecs.starts_with("avc"));
    }

    #[test]
    fn video_chain_falls_back_or_fails() {
        let p = play(vec![stream(32, "avc1"), stream(16, "avc1")], vec![]);
        let chain = vec![pref(127, "auto")];
        assert_eq!(
            p.pick_video_chain(&chain, "nearest").unwrap().id,
            32,
            "全部没命中时回退到可用的最高档"
        );
        assert!(
            p.pick_video_chain(&chain, "fail").is_none(),
            "失败策略下不应退回任何档位"
        );
    }

    #[test]
    fn audio_chain_takes_the_first_available_kind() {
        let p = play_with_special(
            vec![stream(80, "avc1")],
            vec![stream(30280, "mp4a")],
            Some(vec![stream(30250, "ec-3")]),
            Some(vec![stream(30251, "fLaC")]),
        );

        // 第 1 优先 Hi-Res → 命中 flac
        assert_eq!(
            p.pick_audio_chain(&["flac".to_string(), "auto".to_string()])
                .unwrap()
                .id,
            30251
        );
        // 第 1 优先杜比 → 命中 dolby
        assert_eq!(
            p.pick_audio_chain(&["dolby".to_string(), "auto".to_string()])
                .unwrap()
                .id,
            30250
        );
    }

    #[test]
    fn audio_chain_skips_missing_kinds_and_falls_back_to_normal() {
        let p = play(vec![stream(80, "avc1")], vec![stream(30280, "mp4a")]);
        // 无损不存在 → 落到普通音轨
        assert_eq!(
            p.pick_audio_chain(&["flac".to_string(), "normal".to_string()])
                .unwrap()
                .id,
            30280
        );
        // 链上全是拿不到的 → 仍然退回普通音轨，不会没有音轨
        assert_eq!(
            p.pick_audio_chain(&["flac".to_string(), "dolby".to_string()])
                .unwrap()
                .id,
            30280
        );
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
        let picked = p.pick_video(80, "avc").unwrap();
        assert_eq!(picked.id, 80);
        assert!(picked.codecs.starts_with("avc"));
    }

    #[test]
    fn downgrades_when_requested_quality_unavailable() {
        let p = play(vec![stream(32, "avc1"), stream(64, "avc1")], vec![]);
        assert_eq!(
            p.pick_video(120, "avc").unwrap().id,
            64,
            "应降级到可用的最高档"
        );
        assert_eq!(p.best_quality(), 64);
    }

    /// 回归测试：编码偏好不能跨清晰度档位生效。
    ///
    /// 曾经为了优先 avc 而跳过更高的 HEVC 档，导致请求杜比视界（126）时
    /// 只拿到 1080P+（112）。
    #[test]
    fn higher_quality_beats_codec_preference() {
        let p = play(
            vec![stream(126, "hvc1.2.4.L120.90"), stream(112, "avc1.640032")],
            vec![],
        );
        let picked = p.pick_video(126, "avc").unwrap();
        assert_eq!(picked.id, 126, "应取最高档而不是降级去用 avc");
        assert!(picked.codecs.starts_with("hvc"));
    }

    #[test]
    fn codec_preference_applies_within_same_quality() {
        let p = play(
            vec![
                stream(126, "hvc1.2.4.L120.90"),
                stream(126, "dvh1.05.06"),
                stream(112, "avc1.640032"),
            ],
            vec![],
        );
        let picked = p.pick_video(126, "auto").unwrap();
        assert_eq!(picked.id, 126);
        assert!(
            picked.codecs.starts_with("hvc"),
            "关掉编码偏好时取同档第一条，实际: {}",
            picked.codecs
        );
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
