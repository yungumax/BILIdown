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
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert(REFERER, HeaderValue::from_static(REFERER_VALUE));
        headers.insert(ORIGIN, HeaderValue::from_static("https://www.bilibili.com"));
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8"),
        );

        let jar = Arc::new(Jar::default());
        let http = Client::builder()
            .default_headers(headers)
            .cookie_provider(jar.clone())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(180))
            .build()?;

        Ok(Self {
            http,
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
        let url = Url::parse(REFERER_VALUE)
            .map_err(|e| BiliError::InvalidInput(format!("URL 非法: {e}")))?;
        self.add_cookie(&format!("SESSDATA={}", sessdata.trim()), &url);
        Ok(())
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

    async fn fetch_json_inner<T: DeserializeOwned>(&self, url: &str, lenient: bool) -> Result<T> {
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
            return envelope
                .data
                .ok_or_else(|| BiliError::Decode("接口返回 code=0 但缺少 data 字段".into()));
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
