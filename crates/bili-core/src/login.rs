//! 扫码登录：生成二维码 → 轮询状态 → 落地登录 Cookie。
//!
//! 流程走 B 站官方 passport 接口：
//!   1. `qrcode/generate` 拿到二维码内容与 `qrcode_key`
//!   2. `qrcode/poll` 轮询，`data.code` 表示状态（0 成功 / 86090 已扫码待确认 / 86038 过期）
//!   3. 成功时 `data.url` 的 query 中带着 SESSDATA 等 Cookie，解析后即为登录态

use crate::client::BiliClient;
use crate::error::{BiliError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use url::Url;

const API_QR_GENERATE: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/generate";
const API_QR_POLL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/poll";

/// 二维码有效期约 180 秒。
pub const DEFAULT_MAX_WAIT: Duration = Duration::from_secs(180);
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Deserialize)]
pub struct QrCodeInfo {
    /// 二维码内容，也可在已登录的浏览器直接打开
    pub url: String,
    pub qrcode_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PollData {
    /// 0=登录成功 86038=二维码已过期 86090=已扫码待确认 86101=尚未扫码
    #[serde(default)]
    pub code: i32,
    #[serde(default)]
    pub message: String,
    /// 登录成功时是带 Cookie 的跳转地址
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginState {
    /// 等待扫码
    Pending,
    /// 已扫码，等待手机上确认
    Scanned,
    /// 二维码过期
    Expired,
    /// 登录成功
    Confirmed,
}

impl PollData {
    pub fn state(&self) -> LoginState {
        match self.code {
            0 => LoginState::Confirmed,
            86090 => LoginState::Scanned,
            86038 => LoginState::Expired,
            _ => LoginState::Pending,
        }
    }
}

/// 登录态 Cookie 集合。除 SESSDATA 外，其余字段用于维持完整会话。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cookies {
    #[serde(default)]
    pub sessdata: String,
    #[serde(default)]
    pub bili_jct: String,
    #[serde(default)]
    pub dede_user_id: String,
    #[serde(default)]
    pub dede_user_id_ck_md5: String,
    #[serde(default)]
    pub sid: String,
    /// 登录后补全的昵称，仅用于显示
    #[serde(default)]
    pub uname: String,
    #[serde(default)]
    pub saved_at: u64,
}

impl Cookies {
    /// 从 `poll` 成功返回的跳转地址中提取 Cookie（query 值已是解码后的原文）。
    pub fn from_success_url(url: &str) -> Result<Self> {
        let parsed =
            Url::parse(url).map_err(|e| BiliError::Decode(format!("登录跳转地址解析失败: {e}")))?;

        let mut cookies = Cookies::default();
        for (key, value) in parsed.query_pairs() {
            match key.as_ref() {
                "SESSDATA" => cookies.sessdata = value.into_owned(),
                "bili_jct" => cookies.bili_jct = value.into_owned(),
                "DedeUserID" => cookies.dede_user_id = value.into_owned(),
                "DedeUserID__ckMd5" => cookies.dede_user_id_ck_md5 = value.into_owned(),
                "sid" => cookies.sid = value.into_owned(),
                _ => {}
            }
        }

        if !cookies.is_valid() {
            return Err(BiliError::Decode(
                "登录流程已确认，但未能从返回地址中取得 SESSDATA".into(),
            ));
        }
        Ok(cookies)
    }

    pub fn is_valid(&self) -> bool {
        !self.sessdata.trim().is_empty()
    }

    /// 转为可写入 Cookie 头的片段集合。
    pub fn to_pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if !self.sessdata.is_empty() {
            pairs.push(("SESSDATA", self.sessdata.clone()));
        }
        if !self.bili_jct.is_empty() {
            pairs.push(("bili_jct", self.bili_jct.clone()));
        }
        if !self.dede_user_id.is_empty() {
            pairs.push(("DedeUserID", self.dede_user_id.clone()));
        }
        if !self.dede_user_id_ck_md5.is_empty() {
            pairs.push(("DedeUserID__ckMd5", self.dede_user_id_ck_md5.clone()));
        }
        if !self.sid.is_empty() {
            pairs.push(("sid", self.sid.clone()));
        }
        pairs
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(self)
            .map_err(|e| BiliError::Decode(format!("序列化登录态失败: {e}")))?;
        std::fs::write(path, body)?;
        Ok(())
    }

    /// 不存在时返回 `Ok(None)`；存在但损坏时返回错误，避免静默降级成未登录。
    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(path)?;
        let cookies: Cookies = serde_json::from_str(&text)
            .map_err(|e| BiliError::Decode(format!("登录态文件损坏（{}）: {e}", path.display())))?;
        Ok(cookies.is_valid().then_some(cookies))
    }

    /// 返回是否真的删除了文件。
    pub fn remove(path: &Path) -> Result<bool> {
        if path.exists() {
            std::fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// 默认登录态存放路径。
///
/// 当前按用户环境约定放在 D 盘；Tauri 版改为应用数据目录（M2 处理）。
pub fn default_cookie_path() -> PathBuf {
    if let Ok(custom) = std::env::var("BILIDOWN_COOKIE_FILE") {
        if !custom.trim().is_empty() {
            return PathBuf::from(custom);
        }
    }
    PathBuf::from(r"D:\Zcode\_data\bilidown\cookies.json")
}

impl BiliClient {
    /// 申请一个新的登录二维码。
    pub async fn qrcode_generate(&self) -> Result<QrCodeInfo> {
        self.fetch_json(API_QR_GENERATE).await
    }

    /// 查询二维码状态。该接口在未确认时 data 依然存在，因此用宽松模式。
    pub async fn qrcode_poll(&self, qrcode_key: &str) -> Result<PollData> {
        let url = format!("{API_QR_POLL}?qrcode_key={qrcode_key}");
        self.fetch_json_lenient(&url).await
    }

    /// 把登录态写入会话，后续请求自动携带。
    pub fn set_cookies(&self, cookies: &Cookies) -> Result<()> {
        let origin = Url::parse("https://www.bilibili.com/")
            .map_err(|e| BiliError::InvalidInput(format!("URL 非法: {e}")))?;
        for (name, value) in cookies.to_pairs() {
            self.add_cookie(&format!("{name}={value}"), &origin);
        }
        Ok(())
    }
}

/// 轮询直到登录成功、过期或超时。
///
/// `on_state` 仅在状态发生变化时被调用，便于调用方打印进度而不刷屏。
pub async fn wait_for_login<F>(
    client: &BiliClient,
    qrcode_key: &str,
    poll_interval: Duration,
    max_wait: Duration,
    mut on_state: F,
) -> Result<Cookies>
where
    F: FnMut(LoginState),
{
    let started = Instant::now();
    let mut last_state: Option<LoginState> = None;

    loop {
        let poll = client.qrcode_poll(qrcode_key).await?;
        let state = poll.state();

        if last_state != Some(state) {
            on_state(state);
            last_state = Some(state);
        }

        match state {
            LoginState::Confirmed => return Cookies::from_success_url(&poll.url),
            LoginState::Expired => {
                return Err(BiliError::Login("二维码已过期，请重新执行登录".into()))
            }
            LoginState::Pending | LoginState::Scanned => {}
        }

        if started.elapsed() >= max_wait {
            return Err(BiliError::Login(format!(
                "等待扫码超时（{} 秒），请重新执行登录",
                max_wait.as_secs()
            )));
        }

        tokio::time::sleep(poll_interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SUCCESS_URL: &str =
        "https://passport.biligame.com/crossDomain?DedeUserID=12345678\
&DedeUserID__ckMd5=0123456789abcdef&SESSDATA=abc%2Cdef%2Ag%3D%3D&bili_jct=deadbeefcafe&sid=xyz789";

    #[test]
    fn parses_all_cookies_from_success_url() {
        let cookies = Cookies::from_success_url(SAMPLE_SUCCESS_URL).unwrap();
        assert_eq!(
            cookies.sessdata, "abc,def*g==",
            "SESSDATA 需按 URL 解码后保存"
        );
        assert_eq!(cookies.bili_jct, "deadbeefcafe");
        assert_eq!(cookies.dede_user_id, "12345678");
        assert_eq!(cookies.dede_user_id_ck_md5, "0123456789abcdef");
        assert_eq!(cookies.sid, "xyz789");
        assert!(cookies.is_valid());
    }

    #[test]
    fn rejects_url_without_sessdata() {
        let err = Cookies::from_success_url("https://example.com/cb?foo=1").unwrap_err();
        assert!(err.to_string().contains("SESSDATA"), "实际错误: {err}");
    }

    #[test]
    fn maps_poll_codes_to_states() {
        let make = |code| PollData {
            code,
            message: String::new(),
            url: String::new(),
            refresh_token: String::new(),
            timestamp: 0,
        };
        assert_eq!(make(0).state(), LoginState::Confirmed);
        assert_eq!(make(86090).state(), LoginState::Scanned);
        assert_eq!(make(86038).state(), LoginState::Expired);
        assert_eq!(make(86101).state(), LoginState::Pending);
        assert_eq!(make(-1).state(), LoginState::Pending);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join("bilidown-login-test");
        let path = dir.join("cookies.json");
        let _ = std::fs::remove_file(&path);

        assert!(
            Cookies::load(&path).unwrap().is_none(),
            "文件不存在应返回 None"
        );

        let cookies = Cookies::from_success_url(SAMPLE_SUCCESS_URL).unwrap();
        cookies.save(&path).unwrap();

        let loaded = Cookies::load(&path).unwrap().expect("应能读回登录态");
        assert_eq!(loaded.sessdata, cookies.sessdata);
        assert_eq!(loaded.to_pairs().len(), 5);

        assert!(Cookies::remove(&path).unwrap());
        assert!(!Cookies::remove(&path).unwrap(), "重复删除应返回 false");
    }

    /// 联网测试：验证二维码申请与轮询接口可用。
    /// 需要网络，默认跳过，手动执行：`cargo test -p bili-core -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "需要访问 B 站接口"]
    async fn live_qrcode_generate_and_poll() {
        let client = BiliClient::new().unwrap();
        client.warmup().await.unwrap();

        let qr = client.qrcode_generate().await.expect("应能申请到二维码");
        assert!(
            qr.url.starts_with("https://"),
            "二维码内容应为链接: {}",
            qr.url
        );
        assert!(!qr.qrcode_key.is_empty(), "应返回 qrcode_key");
        println!("二维码内容: {}", qr.url);

        let poll = client
            .qrcode_poll(&qr.qrcode_key)
            .await
            .expect("轮询应成功");
        println!("轮询返回状态: code={} message={}", poll.code, poll.message);
        assert_eq!(
            poll.state(),
            LoginState::Pending,
            "没人扫码时应为待扫码状态，实际 code={}",
            poll.code
        );
    }
}
