//! BILIdown 命令行原型：BV 号 → 解析 → DASH 下载 → ffmpeg 合成 mp4，并支持扫码登录。

use anyhow::{anyhow, bail, Context, Result};
use bili_core::api::{quality_name, AudioKind, VideoInfo};
use bili_core::download::{download, DownloadOptions, Progress, ProgressFn};
use bili_core::ffmpeg;
use bili_core::login::{
    default_cookie_path, wait_for_login, Cookies, LoginState, DEFAULT_MAX_WAIT,
    DEFAULT_POLL_INTERVAL,
};
use bili_core::parser::{is_short_link, parse_target, Target};
use bili_core::BiliClient;
use clap::Parser;
use qrcode::render::unicode;
use qrcode::QrCode;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(
    name = "bili-cli",
    version,
    about = "BILIdown M1 原型：B 站视频下载（BV 号 → DASH 下载 → ffmpeg 合成 mp4）"
)]
struct Cli {
    /// BV 号 / av 号 / 视频链接 / b23.tv 短链（仅做登录 / 登出时可不填）
    input: Option<String>,

    /// 扫码登录：用 B 站手机客户端扫描终端里的二维码
    #[arg(long)]
    login: bool,

    /// 退出登录：删除已保存的登录态文件
    #[arg(long)]
    logout: bool,

    /// 登录态文件路径（默认 D:\Zcode\_data\bilidown\cookies.json）
    #[arg(long, value_name = "PATH")]
    cookie_file: Option<PathBuf>,

    /// 输出目录
    #[arg(short, long, value_name = "DIR", default_value = ".")]
    out: PathBuf,

    /// 清晰度 qn：16=360P 32=480P 64=720P 80=1080P 112=1080P+ 120=4K
    #[arg(short, long, default_value_t = 80)]
    quality: u32,

    /// 音频轨：normal / dolby / flac
    #[arg(long, default_value = "normal")]
    audio: String,

    /// 分片并发数
    #[arg(long, default_value_t = 4)]
    concurrency: usize,

    /// 分片大小（字节）
    #[arg(long, default_value_t = 4 * 1024 * 1024)]
    chunk_size: u64,

    /// 登录 Cookie 中的 SESSDATA，用于 1080P 及以上清晰度
    #[arg(long, env = "BILI_SESSDATA")]
    sessdata: Option<String>,

    /// 保留音视频分轨文件
    #[arg(long)]
    keep: bool,

    /// 指定 ffmpeg 可执行文件路径
    #[arg(long, value_name = "PATH")]
    ffmpeg: Option<PathBuf>,

    /// 打印签名后的请求地址等排查信息
    #[arg(long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cookie_path = cli.cookie_file.clone().unwrap_or_else(default_cookie_path);

    if cli.logout {
        if Cookies::remove(&cookie_path)? {
            println!("已退出登录，删除登录态: {}", cookie_path.display());
        } else {
            println!("当前没有已保存的登录态（{}）", cookie_path.display());
        }
        return Ok(());
    }

    let audio_kind = AudioKind::parse(&cli.audio)
        .ok_or_else(|| anyhow!("--audio 只支持 normal / dolby / flac"))?;

    let client = BiliClient::new()?;

    // 登录态优先级：命令行 SESSDATA > 已保存的登录态文件
    if let Some(sessdata) = cli.sessdata.as_deref() {
        client.set_sessdata(sessdata)?;
        println!("已注入 SESSDATA（来自命令行）");
    } else if let Some(saved) = Cookies::load(&cookie_path)? {
        client.set_cookies(&saved)?;
        let who = if saved.uname.is_empty() {
            format!("mid={}", saved.dede_user_id)
        } else {
            saved.uname.clone()
        };
        println!("已载入登录态: {who}");
    }

    client
        .warmup()
        .await
        .context("访问 B 站首页失败，请检查网络或代理")?;

    if cli.login {
        run_login(&client, &cookie_path).await?;
        if cli.input.is_none() {
            return Ok(());
        }
    }

    let input = cli
        .input
        .clone()
        .ok_or_else(|| anyhow!("请提供 BV 号 / av 号 / 链接，或使用 --login 扫码登录"))?;

    let raw_input = if is_short_link(&input) {
        let resolved = client
            .resolve_redirect(&input)
            .await
            .context("解析 b23.tv 短链失败")?;
        println!("短链解析为: {resolved}");
        resolved
    } else {
        input
    };

    let target = parse_target(&raw_input)?;
    let info: VideoInfo = match &target {
        Target::Bvid(bvid) => client.video_info(bvid).await?,
        Target::Aid(aid) => client.video_info_by_aid(*aid).await?,
    };

    println!("标题   : {}", info.title);
    println!("UP 主  : {}", info.owner.name);
    println!("BV 号  : {}", info.bvid);
    println!("时长   : {}", format_duration(info.duration));
    if info.pages.len() > 1 {
        println!(
            "分 P    : 共 {} 个（M1 仅处理 P1: {}）",
            info.pages.len(),
            info.pages[0].part
        );
    }

    let play = client.playurl(&info.bvid, info.cid, cli.quality).await;
    if cli.debug {
        match client
            .playurl_signed_url(&info.bvid, info.cid, cli.quality)
            .await
        {
            Ok(url) => println!("[调试] 签名地址: {url}"),
            Err(e) => println!("[调试] 生成签名地址失败: {e}"),
        }
    }
    let play = play?;
    if play.dash.is_none() {
        bail!("该内容未返回 DASH 流（可能是番剧/课程或需登录），M1 暂只支持普通投稿视频");
    }

    let video = play
        .pick_video(cli.quality, true)
        .ok_or_else(|| anyhow!("未找到可用视频流"))?;
    let audio = play
        .pick_audio(audio_kind)
        .ok_or_else(|| anyhow!("未找到可用音频流"))?;

    println!(
        "清晰度 : {} (id={})  编码 {}  {}x{}",
        quality_name(video.id),
        video.id,
        video.codecs,
        video.width,
        video.height
    );
    if video.id < cli.quality {
        println!(
            "         注意：请求的 {} 未获授权，已自动降级",
            quality_name(cli.quality)
        );
    }
    println!(
        "音频   : id={}  {} kbps  {}",
        audio.id,
        audio.bandwidth / 1000,
        audio.codecs
    );

    let work_dir = cli.out.join(".bilitmp").join(&info.bvid);
    tokio::fs::create_dir_all(&work_dir).await?;
    let video_path = work_dir.join("video.m4s");
    let audio_path = work_dir.join("audio.m4s");

    let opts = DownloadOptions {
        concurrency: cli.concurrency,
        chunk_size: cli.chunk_size,
        ..Default::default()
    };

    println!("下载视频流 ...");
    download(
        &client.http,
        &video.base_url,
        &video.backup_url,
        &video_path,
        &opts,
        make_progress("视频"),
    )
    .await?;
    end_progress_line();

    println!("下载音频流 ...");
    download(
        &client.http,
        &audio.base_url,
        &audio.backup_url,
        &audio_path,
        &opts,
        make_progress("音频"),
    )
    .await?;
    end_progress_line();

    let ffmpeg_bin = ffmpeg::find_ffmpeg(cli.ffmpeg.as_deref()).ok_or_else(|| {
        anyhow!("未找到 ffmpeg：请安装到 PATH，或用 --ffmpeg 指定路径（正式版将内置 sidecar）")
    })?;
    match ffmpeg::probe_version(&ffmpeg_bin).await {
        Ok(version) => println!("ffmpeg : {version}"),
        Err(e) => println!("ffmpeg : 无法探测版本（{e}），继续尝试合成"),
    }

    tokio::fs::create_dir_all(&cli.out).await?;
    let out_file = cli
        .out
        .join(format!("{}.mp4", sanitize_filename(&info.title)));
    println!("合成   : -> {}", out_file.display());
    ffmpeg::merge_video_audio(&ffmpeg_bin, &video_path, &audio_path, &out_file).await?;

    if !cli.keep {
        tokio::fs::remove_dir_all(&work_dir).await.ok();
    }

    let size = tokio::fs::metadata(&out_file).await?.len();
    println!("\n完成: {}  ({})", out_file.display(), human_bytes(size));
    Ok(())
}

fn make_progress(label: &'static str) -> ProgressFn {
    Arc::new(move |p: Progress| {
        let percent = if p.total > 0 {
            p.downloaded as f64 / p.total as f64 * 100.0
        } else {
            0.0
        };
        print!(
            "\r  [{label}] {percent:>5.1}%  {} / {}  {}/s        ",
            human_bytes(p.downloaded),
            human_bytes(p.total),
            human_bytes(p.speed_bps as u64),
        );
        let _ = std::io::stdout().flush();
    })
}

fn end_progress_line() {
    println!();
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn format_duration(seconds: u64) -> String {
    let (h, m, s) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// 文件名清洗：替换 Windows 非法字符并限长。
fn sanitize_filename(name: &str) -> String {
    let mut cleaned: String = name
        .chars()
        .map(|c| {
            if r#"\/:*?"<>|"#.contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    cleaned = cleaned.trim().trim_end_matches('.').to_string();
    if cleaned.chars().count() > 120 {
        cleaned = cleaned.chars().take(120).collect();
    }
    if cleaned.is_empty() {
        cleaned = "video".to_string();
    }
    cleaned
}

/// 扫码登录：打印二维码 → 轮询状态 → 验证并保存登录态。
async fn run_login(client: &BiliClient, cookie_path: &Path) -> Result<Cookies> {
    println!("正在申请登录二维码 ...");
    let qr = client.qrcode_generate().await.context("申请二维码失败")?;

    println!("\n请用 B 站手机客户端（我的 → 扫一扫）扫描下面的二维码：\n");
    match render_qr(&qr.url) {
        Ok(art) => println!("{art}"),
        Err(e) => println!("（二维码渲染失败: {e}）"),
    }
    println!(
        "二维码有效期约 3 分钟。若终端显示异常，可在已登录 B 站的浏览器里打开同一链接完成授权："
    );
    println!("  {}\n", qr.url);
    println!("等待扫码中 ...（在手机上确认后会自动继续，Ctrl+C 可取消）");

    let cookies = wait_for_login(
        client,
        &qr.qrcode_key,
        DEFAULT_POLL_INTERVAL,
        DEFAULT_MAX_WAIT,
        |state| match state {
            LoginState::Pending => {}
            LoginState::Scanned => println!("已扫码，请在手机上点击确认 ..."),
            LoginState::Expired => println!("二维码已过期。"),
            LoginState::Confirmed => println!("已确认，正在获取登录态 ..."),
        },
    )
    .await?;

    client.set_cookies(&cookies)?;
    // 带上登录态重新取一次 wbi 密钥，同时验证 Cookie 是否真的生效
    if let Err(e) = client.refresh_wbi_keys().await {
        println!("警告：刷新 wbi 密钥失败（{e}）");
    }

    let mut cookies = cookies;
    match client.nav().await {
        Ok(nav) if nav.is_login => {
            cookies.uname = nav.uname.clone();
            println!("登录成功: {} (mid={})", nav.uname, nav.mid);
            if nav.vip_status > 0 {
                let label = if nav.vip_label.is_empty() {
                    "有效"
                } else {
                    nav.vip_label.as_str()
                };
                println!("大会员: {label} —— 可下载 1080P+ / 4K / HDR / 杜比 / Hi-Res");
            } else {
                println!("大会员: 未开通（最高 1080P）");
            }
        }
        Ok(_) => println!("警告：Cookie 已保存，但接口显示未登录，登录态可能已失效"),
        Err(e) => println!("警告：验证登录态失败（{e}）"),
    }

    cookies.saved_at = now_secs();
    cookies.save(cookie_path)?;
    println!("登录态已保存: {}", cookie_path.display());
    println!("提示：该文件等同于账号凭据，请勿分享，也不要提交到代码仓库。");
    Ok(cookies)
}

/// 用半块字符在终端渲染二维码；深色终端下做反色处理以便扫描。
fn render_qr(content: &str) -> Result<String> {
    let code = QrCode::new(content.as_bytes()).map_err(|e| anyhow!("生成二维码失败: {e}"))?;
    Ok(code
        .render::<unicode::Dense1x2>()
        .quiet_zone(true)
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .build())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_windows_reserved_characters() {
        assert_eq!(
            sanitize_filename("a/b\\c:d*e?f\"g<h>i|j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
    }

    #[test]
    fn truncates_overlong_names() {
        let long = "字".repeat(200);
        assert_eq!(sanitize_filename(&long).chars().count(), 120);
    }

    #[test]
    fn falls_back_when_name_is_empty() {
        assert_eq!(sanitize_filename("   "), "video");
    }

    #[test]
    fn formats_duration() {
        assert_eq!(format_duration(59), "0:59");
        assert_eq!(format_duration(605), "10:05");
        assert_eq!(format_duration(3671), "1:01:11");
    }
}
