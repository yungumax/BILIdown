//! HTTP 会话层：统一请求头伪装、Cookie 管理、wbi 密钥获取与通用 JSON 请求。

use crate::api::ApiEnvelope;
use crate::error::{explain_code, BiliError, Result};
use crate::login::Cookies;
use crate::wbi::{key_from_url, WbiKeys};
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT_LANGUAGE, ORIGIN, REFERER, USER_AGENT};
use reqwest::{Client, Url};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// 必须伪装成真实浏览器 UA，否则 playurl 等接口会返回风控页。
pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
pub const REFERER_VALUE: &str = "https://www.bilibili.com/";
const WBI_TTL: Duration = Duration::from_secs(1800);

/// 读取会话 Cookie 时探测的站点。
const JAR_ORIGINS: [&str; 4] = [
    "https://www.bilibili.com/",
    "https://api.bilibili.com/",
    "https://passport.bilibili.com/",
    "https://account.bilibili.com/",
];

pub struct BiliClient {
    pub http: Client,
    jar: Arc<Jar>,
    wbi: RwLock<Option<(WbiKeys, Instant)>>,
}

impl BiliClient {
    pub fn new() -> Result<Self> {
        Self::build(None)
    }

    /// 走指定代理（形如 `http://127.0.0.1:7890`）；传空则直连。
    pub fn with_proxy(proxy: Option<&str>) -> Result<Self> {
        Self::build(proxy.filter(|value| !value.trim().is_empty()))
    }

    fn build(proxy: Option<&str>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert(REFERER, HeaderValue::from_static(REFERER_VALUE));
        headers.insert(ORIGIN, HeaderValue::from_static("https://www.bilibili.com"));
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8"),
        );

        let jar = Arc::new(Jar::default());
        let mut builder = Client::builder()
            .default_headers(headers)
            .cookie_provider(jar.clone())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(180));

        if let Some(addr) = proxy {
            let proxy = reqwest::Proxy::all(addr.trim())
                .map_err(|e| BiliError::InvalidInput(format!("代理地址无效: {e}")))?;
            builder = builder.proxy(proxy);
        }

        Ok(Self {
            http: builder.build()?,
            jar,
            wbi: RwLock::new(None),
        })
    }

    /// 向会话中写入一条 Cookie，后续请求自动携带。
    pub fn add_cookie(&self, cookie: &str, url: &Url) {
        self.jar.add_cookie_str(cookie, url);
    }

    /// 从会话 Cookie 罐中取出登录相关 Cookie。
    ///
    /// 扫码登录成功后，poll 响应会以 Set-Cookie 下发 SESSDATA 等。这里按浏览器的语义
    /// **原样保留 Cookie 值**（不做百分号解码），因为浏览器发送 Cookie 时也不会解码。
    pub fn cookies_from_jar(&self) -> Cookies {
        self.cookies_from_jar_with(&[])
    }

    /// `extra_urls` 用于覆盖 Cookie 受路径限制的情况：Cookie 可能只挂在某个具体路径下，
    /// 此时按站点根探测不到，需要用在这次登录流程里真正请求过的地址再探一次。
    pub fn cookies_from_jar_with(&self, extra_urls: &[String]) -> Cookies {
        let mut cookies = Cookies::default();
        for url in self.jar_probe_urls(extra_urls) {
            if let Some(header) = self.jar.cookies(&url) {
                let Ok(text) = header.to_str() else { continue };
                for pair in text.split(';') {
                    if let Some((name, value)) = pair.trim().split_once('=') {
                        cookies.set_if_empty(name.trim(), value.trim().to_string());
                    }
                }
            }
        }
        cookies
    }

    /// 仅用于诊断：列出会话中已有的 Cookie 名称（不含值，避免凭据泄漏）。
    pub fn jar_cookie_names(&self) -> Vec<String> {
        self.jar_cookie_names_with(&[])
    }

    pub fn jar_cookie_names_with(&self, extra_urls: &[String]) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for url in self.jar_probe_urls(extra_urls) {
            if let Some(header) = self.jar.cookies(&url) {
                let Ok(text) = header.to_str() else { continue };
                for pair in text.split(';') {
                    let Some((name, _)) = pair.trim().split_once('=') else {
                        continue;
                    };
                    let name = name.trim().to_string();
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
        names
    }

    fn jar_probe_urls(&self, extra_urls: &[String]) -> Vec<Url> {
        JAR_ORIGINS
            .iter()
            .map(|origin| origin.to_string())
            .chain(extra_urls.iter().cloned())
            .filter_map(|candidate| Url::parse(&candidate).ok())
            .collect()
    }

    /// 注入 SESSDATA；1080P 及以上清晰度需要登录态。
    pub fn set_sessdata(&self, sessdata: &str) -> Result<()> {
        self.set_cookies(&Cookies {
            sessdata: sessdata.trim().to_string(),
            ..Default::default()
        })
    }

    /// 访问首页拿到 buvid3 等风控 Cookie，可显著降低被拦截概率。
    pub async fn warmup(&self) -> Result<()> {
        let resp = self.http.get(REFERER_VALUE).send().await?;
        // 只关心 Cookie 落地，不解析正文
        let _ = resp.bytes().await;
        Ok(())
    }

    /// 跟随重定向拿到最终地址（用于 b23.tv 短链）。
    pub async fn resolve_redirect(&self, url: &str) -> Result<String> {
        let resp = self.http.get(url).send().await?;
        Ok(resp.url().to_string())
    }

    /// 取得（并缓存）wbi 密钥，过期或签名失败后可强制刷新。
    pub async fn wbi_keys(&self) -> Result<WbiKeys> {
        if let Some((keys, fetched)) = self.wbi.read().await.clone() {
            if fetched.elapsed() < WBI_TTL {
                return Ok(keys);
            }
        }
        self.refresh_wbi_keys().await
    }

    pub async fn refresh_wbi_keys(&self) -> Result<WbiKeys> {
        let nav = self.nav().await?;
        let keys = WbiKeys {
            img_key: key_from_url(&nav.wbi_img.img_url),
            sub_key: key_from_url(&nav.wbi_img.sub_url),
        };
        if keys.img_key.is_empty() || keys.sub_key.is_empty() {
            return Err(BiliError::Decode("nav 接口未返回有效的 wbi 密钥".into()));
        }
        *self.wbi.write().await = Some((keys.clone(), Instant::now()));
        Ok(keys)
    }

    /// 拼出带 wbi 签名的完整请求地址。
    pub async fn signed_url(&self, base: &str, params: Vec<(&str, String)>) -> Result<String> {
        let keys = self.wbi_keys().await?;
        Ok(format!("{base}?{}", keys.sign_query(params)))
    }

    /// 请求接口并解出 `data` 字段；非 0 的 code 会转成带解释的错误。
    pub async fn fetch_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        self.fetch_json_inner(url, false).await
    }

    /// 宽松模式：某些接口未登录时返回非 0 code 但仍带有效 data
    /// （典型是 `nav` 返回 -101 账号未登录，却照样给出 wbi 密钥），此类接口按成功处理。
    pub async fn fetch_json_lenient<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        self.fetch_json_inner(url, true).await
    }

    /// `data` 允许为 null：翻过列表最后一页时这些接口会这样返回（code 仍是 0），
    /// 而它们报的 totalSize 常常**大于实际能取到的条数**（实测音频 296 报数、
    /// 实取 266），所以必然会翻过去一次。调用方把 None 当成"没有更多"。
    pub async fn fetch_json_opt<T: DeserializeOwned>(&self, url: &str) -> Result<Option<T>> {
        self.fetch_envelope(url, true, false).await
    }

    /// 取一页 HTML/文本（图文详情只有页面里带完整内容，接口要风控签名）。
    pub async fn fetch_text(&self, url: &str) -> Result<String> {
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(BiliError::Unavailable(format!(
                "HTTP {status}: {}",
                truncate(&text, 200)
            )));
        }
        Ok(text)
    }

    /// 取二进制（图文里的原图）并带上 Content-Type，便于判断扩展名。
    pub async fn fetch_bytes(&self, url: &str) -> Result<(Vec<u8>, String)> {
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if !status.is_success() {
            return Err(BiliError::Unavailable(format!("HTTP {status}: {url}")));
        }
        let bytes = resp.bytes().await?;
        Ok((bytes.to_vec(), content_type))
    }

    /// 这条视频的字幕清单。番剧用 `ep_id`、普通视频用 `bvid`，都要带 `cid`。
    ///
    /// 先试签名接口（网页播放器用的 `x/player/wbi/v2`），被风控挡住再退回未签名的
    /// `x/player/v2`。两条都通但都是空表就是这条没字幕——这是常态，不算错误。
    pub async fn subtitles(
        &self,
        bvid: Option<&str>,
        ep_id: Option<u64>,
        cid: u64,
    ) -> Result<Vec<crate::subtitle::SubtitleItem>> {
        let mut params: Vec<(&str, String)> = vec![("cid", cid.to_string())];
        if let Some(bvid) = bvid.filter(|b| !b.is_empty()) {
            params.push(("bvid", bvid.to_string()));
        }
        if let Some(ep_id) = ep_id.filter(|e| *e > 0) {
            params.push(("ep_id", ep_id.to_string()));
        }
        // **只信签名接口**（网页播放器用的那个）。未签名的老接口 x/player/v2 虽然能通，
        // 但它会回一份"会话里的字幕"——实测拿到过跟本条视频毫无关系的字幕，
        // 写出去就是把别人的台词贴在视频旁边，比没有字幕更糟。所以不做这个回退。
        let url = self
            .signed_url("https://api.bilibili.com/x/player/wbi/v2", params)
            .await?;
        let resp: crate::subtitle::PlayerSubtitles = self.fetch_json(&url).await?;
        let list = resp.subtitle.map(|s| s.subtitles).unwrap_or_default();
        // AI 字幕是按需生成的：没生成过时地址是空的，这种条目直接丢掉
        Ok(list
            .into_iter()
            .filter(|item| !item.subtitle_url.trim().is_empty())
            .collect())
    }

    /// 取一份字幕内容（清单里的 `subtitle_url` 是协议相对的，这里补 https:）。
    pub async fn subtitle_text(&self, url: &str) -> Result<String> {
        let url = if url.starts_with("//") {
            format!("https:{url}")
        } else {
            url.to_string()
        };
        self.fetch_text(&url).await
    }

    /// 全量弹幕，转成播放器直接读的 XML。
    ///
    /// 主路是网页播放器用的 `seg.so`（protobuf，按段翻页，翻完即全量）；
    /// 任何一段失败就停下用已经拿到的，一条都没拿到再退回 XML 接口——
    /// 那个只给一小部分（实测 91 万弹幕的视频只返回 1200 条），但总比空文件强。
    pub async fn danmaku_full(&self, cid: u64) -> Result<(String, usize, bool)> {
        let mut all = Vec::new();
        for segment in 1..=40u32 {
            let url = format!(
                "https://api.bilibili.com/x/v2/dm/web/seg.so?type=1&oid={cid}&segment_index={segment}"
            );
            let resp = self.http.get(&url).send().await?;
            if !resp.status().is_success() {
                break;
            }
            let bytes = resp.bytes().await?;
            let elems = crate::danmaku::parse_segment(&bytes);
            if elems.is_empty() {
                break;
            }
            all.extend(elems);
        }
        if !all.is_empty() {
            let count = all.len();
            return Ok((crate::danmaku::to_xml(&all), count, false));
        }
        let xml = self.danmaku_xml(cid).await?;
        let count = danmaku_count(&xml);
        Ok((xml, count, true))
    }

    /// 兜底：`comment.bilibili.com/{cid}.xml`，公开接口、不需要登录，
    /// 返回的是**已解压的 XML 文本**，但条数被平台截断（见 [`Self::danmaku_full`]）。
    pub async fn danmaku_xml(&self, cid: u64) -> Result<String> {
        let url = format!("https://comment.bilibili.com/{cid}.xml");
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(BiliError::Unavailable(format!("弹幕接口 HTTP {status}")));
        }
        let bytes = resp.bytes().await?;
        decode_danmaku(&bytes)
    }

    /// pgc（番剧）系接口的信封不同：载荷在 `result` 字段而非 `data`。
    pub async fn fetch_pgc_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            return Err(BiliError::Unavailable(format!(
                "HTTP {status}: {}",
                truncate(&text, 200)
            )));
        }

        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| BiliError::Decode(format!("{e}；响应片段: {}", truncate(&text, 300))))?;

        let code = value.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            let message = value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            return Err(BiliError::Api {
                code: code as i32,
                message: format!(
                    "{} ；请求 {}",
                    explain_code(code as i32, &message),
                    truncate(url, 160)
                ),
            });
        }

        serde_json::from_value::<T>(
            value
                .get("result")
                .cloned()
                .ok_or_else(|| BiliError::Decode("接口返回 code=0 但缺少 result 字段".into()))?,
        )
        .map_err(|e| BiliError::Decode(format!("{e}")))
    }

    async fn fetch_json_inner<T: DeserializeOwned>(&self, url: &str, lenient: bool) -> Result<T> {
        self.fetch_envelope(url, false, lenient)
            .await?
            .ok_or_else(|| BiliError::Decode("接口返回 code=0 但缺少 data 字段".into()))
    }

    /// 解析信封；`allow_null` 决定 `data: null` 是 Ok(None) 还是解码错误。
    async fn fetch_envelope<T: DeserializeOwned>(
        &self,
        url: &str,
        allow_null: bool,
        lenient: bool,
    ) -> Result<Option<T>> {
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            return Err(BiliError::Unavailable(format!(
                "HTTP {status}: {}",
                truncate(&text, 200)
            )));
        }

        let envelope: ApiEnvelope<T> = serde_json::from_str(&text)
            .map_err(|e| BiliError::Decode(format!("{e}；响应片段: {}", truncate(&text, 300))))?;

        if envelope.code == 0 || (lenient && envelope.data.is_some()) {
            if envelope.data.is_none() && !allow_null {
                return Err(BiliError::Decode("接口返回 code=0 但缺少 data 字段".into()));
            }
            return Ok(envelope.data);
        }

        Err(BiliError::Api {
            code: envelope.code,
            message: format!(
                "{} ；请求 {} ；响应 {}",
                explain_code(envelope.code, &envelope.message),
                truncate(url, 160),
                truncate(&text, 200)
            ),
        })
    }
}

pub(crate) fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push_str("...");
        out
    }
}

/// 弹幕接口给的是 **raw deflate**（没有 zlib 头，实测首字节 `0xEC`），
/// reqwest 的 deflate 特性认的是 zlib 头，解不了，所以自己来。
/// 明文 XML（代理重编码、接口改版）与 zlib 两种情况也一并认掉。
pub fn decode_danmaku(bytes: &[u8]) -> Result<String> {
    if bytes.first() == Some(&b'<') {
        return String::from_utf8(bytes.to_vec())
            .map_err(|_| BiliError::Unavailable("弹幕不是 UTF-8".to_string()));
    }
    inflate(bytes, true)
        .or_else(|| inflate(bytes, false))
        .ok_or_else(|| BiliError::Unavailable("弹幕数据解不开（既不是明文也不是 deflate）".to_string()))
}

fn inflate(bytes: &[u8], raw: bool) -> Option<String> {
    use std::io::Read;
    let mut out = Vec::new();
    let ok = if raw {
        flate2::read::DeflateDecoder::new(bytes).read_to_end(&mut out).is_ok()
    } else {
        flate2::read::ZlibDecoder::new(bytes).read_to_end(&mut out).is_ok()
    };
    if !ok || out.is_empty() {
        return None;
    }
    String::from_utf8(out).ok()
}

/// 弹幕条数：`<d p="...">内容</d>` 的个数，用来在日志里说清楚存了多少条。
pub fn danmaku_count(xml: &str) -> usize {
    xml.matches("<d p=").count()
}

#[cfg(test)]
mod danmaku_tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?><i><chatserver>chat.bilibili.com</chatserver><d p="1.5,1,25,16777215,0,0,0,0">第一条</d><d p="2.5,1,25,16777215,0,0,0,0">第二条</d></i>"#;

    fn raw_deflate(text: &str) -> Vec<u8> {
        use flate2::write::DeflateEncoder;
        use std::io::Write;
        let mut out = Vec::new();
        let mut enc = DeflateEncoder::new(&mut out, flate2::Compression::default());
        enc.write_all(text.as_bytes()).unwrap();
        enc.finish().unwrap();
        out
    }

    /// 三种输入都要认：B 站实际给的 raw deflate、明文、zlib；乱码则报错。
    #[test]
    fn decode_accepts_raw_deflate_plain_and_zlib() {
        let packed = raw_deflate(XML);
        assert_ne!(packed[0], 0x78, "0x78 开头才是 zlib 流，raw deflate 没有头");
        assert_eq!(decode_danmaku(&packed).unwrap(), XML);
        assert_eq!(decode_danmaku(XML.as_bytes()).unwrap(), XML);

        let zlibbed = {
            use flate2::write::ZlibEncoder;
            use std::io::Write;
            let mut out = Vec::new();
            let mut enc = ZlibEncoder::new(&mut out, flate2::Compression::default());
            enc.write_all(XML.as_bytes()).unwrap();
            enc.finish().unwrap();
            out
        };
        assert_eq!(decode_danmaku(&zlibbed).unwrap(), XML);

        assert!(decode_danmaku(&[0xff, 0x00, 0x12, 0x34]).is_err());
    }

    #[test]
    fn counts_entries() {
        assert_eq!(danmaku_count(XML), 2);
        assert_eq!(danmaku_count(r#"<?xml version="1.0"?><i></i>"#), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie_header_for(client: &BiliClient, url: &str) -> String {
        let url = Url::parse(url).unwrap();
        client
            .jar
            .cookies(&url)
            .and_then(|value| value.to_str().ok().map(str::to_string))
            .unwrap_or_default()
    }

    /// 回归测试：登录态安装后必须能被 api 子域读到。
    ///
    /// 曾经的缺陷是把 Cookie 按 host-only 只挂在 www 上，导致登录态校验接口
    /// （api.bilibili.com）收不到 Cookie，扫码确认后仍被判为未登录。
    #[test]
    fn installed_cookies_reach_api_subdomain() {
        let client = BiliClient::new().unwrap();
        client
            .set_cookies(&Cookies {
                sessdata: "sess_value".into(),
                bili_jct: "jct_value".into(),
                ..Default::default()
            })
            .unwrap();

        let api = cookie_header_for(&client, "https://api.bilibili.com/x/web-interface/nav");
        assert!(
            api.contains("SESSDATA=sess_value"),
            "api 子域应收到 SESSDATA: {api}"
        );
        assert!(
            api.contains("bili_jct=jct_value"),
            "api 子域应收到 bili_jct: {api}"
        );

        let www = cookie_header_for(&client, "https://www.bilibili.com/");
        assert!(www.contains("SESSDATA=sess_value"), "www 也应收到: {www}");
    }

    #[test]
    fn sessdata_helper_installs_domain_wide_cookie() {
        let client = BiliClient::new().unwrap();
        client.set_sessdata(" manual_value ").unwrap();
        let api = cookie_header_for(&client, "https://api.bilibili.com/x/web-interface/nav");
        assert!(
            api.contains("SESSDATA=manual_value"),
            "应去除首尾空格并覆盖 api 子域: {api}"
        );
    }
}
