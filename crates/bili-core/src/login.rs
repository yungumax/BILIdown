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
    /// 从 `poll` 成功返回的地址中提取 Cookie。
    ///
    /// 各版本流程的下发位置不一致：可能直接在 query、在 fragment、或被整段编码在
    /// `gourl` 一类参数里，因此这里做多层扫描 + 百分号解码。
    pub fn from_success_url(url: &str) -> Result<Self> {
        let mut cookies = Cookies::default();
        harvest(url, &mut cookies, 3);

        if !cookies.is_valid() {
            return Err(BiliError::Decode(
                "登录流程已确认，但未能从返回地址中取得 SESSDATA".into(),
            ));
        }
        Ok(cookies)
    }

    /// 仅在该字段尚未取到值时写入，先到先得。
    pub(crate) fn set_if_empty(&mut self, name: &str, value: String) {
        match name {
            "SESSDATA" if self.sessdata.is_empty() => self.sessdata = value,
            "bili_jct" if self.bili_jct.is_empty() => self.bili_jct = value,
            "DedeUserID" if self.dede_user_id.is_empty() => self.dede_user_id = value,
            "DedeUserID__ckMd5" if self.dede_user_id_ck_md5.is_empty() => {
                self.dede_user_id_ck_md5 = value
            }
            "sid" if self.sid.is_empty() => self.sid = value,
            _ => {}
        }
    }

    /// 同一份登录态的“解码变体”：Cookie 罐原样保留，地址解析则已解码，
    /// 两者哪个能被服务端接受由调用方实际验证决定。
    pub fn decoded_variant(&self) -> Self {
        let mut variant = self.clone();
        variant.sessdata = percent_decode(&self.sessdata);
        variant.bili_jct = percent_decode(&self.bili_jct);
        variant
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

/// 从任意文本中扫出登录 Cookie。
///
/// 按 `&`/`;` 切分键值对，键名允许带有前置噪声（URL 前缀、`#` 片段），
/// 值里的嵌套 URL 会再递归解析一层，覆盖“Cookie 被编码在 gourl 参数里”的形态。
fn harvest(text: &str, cookies: &mut Cookies, depth: u8) {
    if depth == 0 || text.is_empty() {
        return;
    }

    let normalized = text.replace(';', "&");
    for (key, value) in url::form_urlencoded::parse(normalized.as_bytes()) {
        let name = key
            .rsplit(['?', '#', '/'])
            .next()
            .unwrap_or(&key)
            .to_string();
        cookies.set_if_empty(&name, value.to_string());

        if value.contains('=') {
            harvest(&value, cookies, depth - 1);
        }
    }
}

/// 百分号解码；不含 `%` 时原样返回。
fn percent_decode(value: &str) -> String {
    if !value.contains('%') {
        return value.to_string();
    }
    url::form_urlencoded::parse(format!("v={value}").as_bytes())
        .next()
        .map(|(_, decoded)| decoded.into_owned())
        .unwrap_or_else(|| value.to_string())
}

/// poll 接口地址。登录流程中真正请求过的地址，也可用于按路径探测 Cookie。
pub fn poll_url(qrcode_key: &str) -> String {
    format!("{API_QR_POLL}?qrcode_key={qrcode_key}")
}

/// 默认登录态存放路径。
///
/// 开发者可用 `BILIDOWN_COOKIE_FILE` 环境变量覆盖（沙箱/多实例测试用）；
/// 正式安装版默认落 `%APPDATA%\com.yungumax.bilidown\`，与用户数据一起，
/// 不依赖任何 D 盘路径，也不进安装目录（卸载不删数据）。
pub fn default_cookie_path() -> PathBuf {
    if let Ok(custom) = std::env::var("BILIDOWN_COOKIE_FILE") {
        if !custom.trim().is_empty() {
            return PathBuf::from(custom);
        }
    }
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata)
        .join("com.yungumax.bilidown")
        .join("cookies.json")
}

impl BiliClient {
    /// 申请一个新的登录二维码。
    pub async fn qrcode_generate(&self) -> Result<QrCodeInfo> {
        self.fetch_json(API_QR_GENERATE).await
    }

    /// 查询二维码状态。该接口在未确认时 data 依然存在，因此用宽松模式。
    pub async fn qrcode_poll(&self, qrcode_key: &str) -> Result<PollData> {
        let url = poll_url(qrcode_key);
        self.fetch_json_lenient(&url).await
    }

    /// 把登录态写入会话，后续请求自动携带。
    ///
    /// 必须显式声明 `Domain=.bilibili.com`：登录态实际校验发生在 `api.bilibili.com`，
    /// 若按 host-only 只挂在 `www.bilibili.com` 下，api 子域收不到，会被判为未登录。
    pub fn set_cookies(&self, cookies: &Cookies) -> Result<()> {
        let origin = Url::parse("https://www.bilibili.com/")
            .map_err(|e| BiliError::InvalidInput(format!("URL 非法: {e}")))?;
        for (name, value) in cookies.to_pairs() {
            self.add_cookie(
                &format!("{name}={value}; Domain=.bilibili.com; Path=/"),
                &origin,
            );
        }
        Ok(())
    }
}

/// 登录确认后取回并验证登录态。
///
/// Cookie 可能由 poll 响应的 Set-Cookie 下发，也可能写在跳转地址里，且编码形态不定：
/// - Set-Cookie 的值要**原样**回送（浏览器语义）
/// - 地址里的值经过 URL 编码，需要解码后回送
///
/// 因此这里把可能的形态都试装一遍，用 `nav` 接口实测，取第一个真正生效的。
/// 扫码确认后落地登录态：跟随成功地址、尝试多种 Cookie 形态，并用 `nav` 实测。
///
/// 供需要自己控制轮询节奏的调用方使用（如桌面端的登录弹窗），命令行侧由
/// [`wait_for_login`] 内部调用。
pub async fn confirm(client: &BiliClient, qrcode_key: &str, success_url: &str) -> Result<Cookies> {
    let probes: Vec<String> = [poll_url(qrcode_key), success_url.to_string()]
        .into_iter()
        .filter(|u| !u.is_empty())
        .collect();

    // 浏览器在扫码成功后会跳转到该地址完成凭据投递（ticket 换取跨域 Cookie），
    // 这里先忠实跟随一次，再读会话。
    if !success_url.is_empty() {
        let _ = client.http.get(success_url).send().await;
    }

    for cookies in collect_candidates(client, &probes, success_url) {
        if verify_cookies(client, &cookies).await {
            return Ok(cookies);
        }
    }

    let names = client.jar_cookie_names_with(&probes);
    Err(BiliError::Login(format!(
        "登录已确认，但未能取得可用的登录态；跳转地址: {}；会话已有 Cookie: [{}]",
        if success_url.is_empty() {
            "<空>"
        } else {
            success_url
        },
        if names.is_empty() {
            "无".to_string()
        } else {
            names.join(", ")
        }
    )))
}

/// 汇总裁剪出所有可能的登录态形态（按可信度排序，去重）。
fn collect_candidates(client: &BiliClient, probes: &[String], success_url: &str) -> Vec<Cookies> {
    let mut candidates: Vec<Cookies> = Vec::new();

    let from_jar = client.cookies_from_jar_with(probes);
    push_candidate(&mut candidates, from_jar.clone());
    if let Ok(from_url) = Cookies::from_success_url(success_url) {
        push_candidate(&mut candidates, from_url.decoded_variant());
    }
    push_candidate(&mut candidates, from_jar.decoded_variant());

    candidates
}

/// 装上候选登录态并用 `nav` 接口实测是否真的登录成功。
async fn verify_cookies(client: &BiliClient, cookies: &Cookies) -> bool {
    client.set_cookies(cookies).is_ok() && matches!(client.nav().await, Ok(nav) if nav.is_login)
}

fn push_candidate(candidates: &mut Vec<Cookies>, cookies: Cookies) {
    if cookies.is_valid() && !candidates.iter().any(|c| c.sessdata == cookies.sessdata) {
        candidates.push(cookies);
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
            LoginState::Confirmed => return confirm(client, qrcode_key, &poll.url).await,
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
    fn parses_cookies_placed_in_fragment() {
        let cookies = Cookies::from_success_url(
            "https://www.bilibili.com/#SESSDATA=frag%2Cvalue&bili_jct=j1",
        )
        .unwrap();
        assert_eq!(cookies.sessdata, "frag,value");
        assert_eq!(cookies.bili_jct, "j1");
    }

    #[test]
    fn parses_cookies_nested_in_encoded_gourl() {
        // Cookie 被整段编码在 gourl 参数里（再嵌套一层百分号编码）
        let url = "https://passport.bilibili.com/cb?gourl=https%3A%2F%2Fwww.bilibili.com%2F%3FSESSDATA%3Dnested%252Cval%26bili_jct%3Dnested_jct";
        let cookies = Cookies::from_success_url(url).unwrap();
        assert_eq!(cookies.sessdata, "nested,val");
        assert_eq!(cookies.bili_jct, "nested_jct");
    }

    #[test]
    fn decoded_variant_only_decodes_encoded_values() {
        let plain = Cookies {
            sessdata: "plain,value*ok==".into(),
            ..Default::default()
        };
        assert_eq!(
            plain.decoded_variant().sessdata,
            "plain,value*ok==",
            "已是明文时不应改动"
        );

        let encoded = Cookies {
            sessdata: "abc%2Cdef%2Ag%3D%3D".into(),
            ..Default::default()
        };
        assert_eq!(encoded.decoded_variant().sessdata, "abc,def*g==");
    }

    #[test]
    fn jar_reading_keeps_values_verbatim() {
        let client = BiliClient::new().unwrap();
        let origin = Url::parse("https://www.bilibili.com/").unwrap();
        client.add_cookie("SESSDATA=abc%2Cdef%2Ag%3D%3D", &origin);
        client.add_cookie("bili_jct=token-123", &origin);

        let cookies = client.cookies_from_jar();
        assert!(cookies.is_valid());
        assert_eq!(
            cookies.sessdata, "abc%2Cdef%2Ag%3D%3D",
            "Cookie 罐的值应按浏览器语义原样保留"
        );
        assert_eq!(cookies.bili_jct, "token-123");
        assert!(client.jar_cookie_names().contains(&"SESSDATA".to_string()));
        assert_eq!(cookies.decoded_variant().sessdata, "abc,def*g==");
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

    /// 联网测试：用磁盘上的真实登录态验证“安装 → nav 校验”这条链路。
    ///
    /// 手动执行：`cargo test -p bili-core -- --ignored live_verify_saved_cookies --nocapture`
    /// 可用 `BILIDOWN_COOKIE_FILE` 指定凭据文件。
    #[tokio::test]
    #[ignore = "需要真实登录态文件"]
    async fn live_verify_saved_cookies() {
        let path = default_cookie_path();
        let Some(cookies) = Cookies::load(&path).expect("凭据文件应能读回") else {
            println!("{} 不存在或无效，跳过", path.display());
            return;
        };
        println!(
            "读入凭据: sessdata 长度={} bili_jct 长度={} mid={}",
            cookies.sessdata.len(),
            cookies.bili_jct.len(),
            cookies.dede_user_id
        );

        let client = BiliClient::new().unwrap();
        client.warmup().await.unwrap();
        client.set_cookies(&cookies).unwrap();

        let names = client.jar_cookie_names();
        println!("会话中的 Cookie 名称: [{}]", names.join(", "));

        let nav = client.nav().await.expect("nav 接口应可访问");
        println!(
            "nav 结果: isLogin={} uname={:?} mid={} vip={}",
            nav.is_login, nav.uname, nav.mid, nav.vip_status
        );
        assert!(nav.is_login, "安装后的登录态应能通过 nav 校验");
    }
}
