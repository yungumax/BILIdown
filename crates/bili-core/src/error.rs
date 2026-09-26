use thiserror::Error;

#[derive(Debug, Error)]
pub enum BiliError {
    #[error("网络请求失败: {0}")]
    Http(#[from] reqwest::Error),

    #[error("文件读写失败: {0}")]
    Io(#[from] std::io::Error),

    #[error("响应格式异常: {0}")]
    Decode(String),

    #[error("B 站接口返回错误 code={code}: {message}")]
    Api { code: i32, message: String },

    #[error("无法识别的输入: {0}")]
    InvalidInput(String),

    #[error("内容不可下载: {0}")]
    Unavailable(String),

    #[error("登录失败: {0}")]
    Login(String),

    #[error("未找到可用清晰度（请求 qn={0}），该内容可能需登录或大会员")]
    QualityNotFound(u32),

    #[error("ffmpeg 不可用: {0}")]
    FfmpegUnavailable(String),

    #[error("ffmpeg 执行失败 (exit={code:?}): {stderr}")]
    FfmpegFailed { code: Option<i32>, stderr: String },

    #[error("下载不完整: 期望 {expected} 字节，实际 {actual} 字节")]
    IncompleteDownload { expected: u64, actual: u64 },
}

pub type Result<T> = std::result::Result<T, BiliError>;

/// 把接口返回的整型错误码翻译成可读原因。
pub fn explain_code(code: i32, raw: &str) -> String {
    let hint = match code {
        -101 => "账号未登录",
        -102 => "账号被封停",
        -111 => "csrf 校验失败",
        -400 => "请求参数错误",
        -403 => "访问权限不足（可能需要登录或大会员）",
        -404 => "资源不存在",
        -509 => "请求过于频繁，触发限流",
        -10403 => "当前账号权限不足，无法访问该清晰度（可能需要大会员）",
        62002 => "稿件不可见（可能已被 UP 主设为私密）",
        62004 => "稿件审核中",
        _ => return raw.to_string(),
    };
    format!("{hint}（原始信息: {raw}）")
}
