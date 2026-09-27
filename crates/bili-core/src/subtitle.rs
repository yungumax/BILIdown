//! 字幕：取 B 站字幕并转成播放器直接读的 SRT。
//!
//! 两段：
//!
//! - 字幕**清单**来自 `x/player/wbi/v2`（要 wbi 签名，和播流接口同一个套路），
//!   番剧用 `ep_id`、普通视频用 `bvid`，都带 `cid`。清单里每条有语言和
//!   `subtitle_url`（协议相对，得补 `https:`）。
//! - 字幕**内容**是 JSON（`{"body":[{"from":1.23,"to":4.56,"content":"…"}]}`），
//!   这里转成 SRT（播放器普遍认，mpv / PotPlayer / VLC 都能挂同名文件）。

use serde::Deserialize;

/// 字幕清单里的一条。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SubtitleItem {
    #[serde(default)]
    pub lan: String,
    #[serde(default)]
    pub lan_doc: String,
    #[serde(default)]
    pub subtitle_url: String,
    /// 0=人工 1=AI 生成
    #[serde(default)]
    pub ai_type: i64,
}

/// `x/player/wbi/v2` 里我们关心的部分。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PlayerSubtitles {
    #[serde(default)]
    pub subtitle: Option<SubtitleList>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SubtitleList {
    #[serde(default)]
    pub subtitles: Vec<SubtitleItem>,
}

/// 字幕内容 JSON。
#[derive(Debug, Clone, Deserialize, Default)]
struct SubtitleBody {
    #[serde(default)]
    body: Vec<SubtitleLine>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct SubtitleLine {
    #[serde(default)]
    from: f64,
    #[serde(default)]
    to: f64,
    #[serde(default)]
    content: String,
}

/// 把 B 站字幕 JSON 转成 SRT。空字幕返回空串（调用方据此决定要不要写文件）。
pub fn srt_from_json(json: &str) -> Result<String, serde_json::Error> {
    let body: SubtitleBody = serde_json::from_str(json)?;
    let mut out = String::with_capacity(body.body.len() * 80);
    let mut index = 0usize;
    for line in body.body {
        let text = line.content.trim();
        if text.is_empty() {
            continue;
        }
        index += 1;
        out.push_str(&index.to_string());
        out.push('\n');
        out.push_str(&format!("{} --> {}\n", srt_time(line.from), srt_time(line.to)));
        // B 站用 \n 表示换行，SRT 里就是真的换行
        out.push_str(&text.replace("\\n", "\n").replace("\r\n", "\n"));
        out.push_str("\n\n");
    }
    Ok(out)
}

/// SRT 的时间戳：`HH:MM:SS,mmm`（逗号，不是点）。
fn srt_time(seconds: f64) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_secs = total_ms / 1000;
    let s = total_secs % 60;
    let m = (total_secs / 60) % 60;
    let h = total_secs / 3600;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

/// 语言标签里的短名（`zh-CN` → `zh`），用来给多语言字幕起后缀。
pub fn lang_tag(item: &SubtitleItem) -> String {
    let raw = if item.lan.trim().is_empty() {
        item.lan_doc.trim()
    } else {
        item.lan.trim()
    };
    let tag: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let tag = tag.trim_matches('-').to_string();
    if tag.is_empty() {
        "sub".to_string()
    } else {
        tag
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_bilibili_json_to_srt() {
        let json = r#"{"font_size":0.4,"body":[
            {"from":1.234,"to":4.5,"content":"第一句"},
            {"from":4.5,"to":3661.007,"content":"第二句\\n换行了"},
            {"from":8.0,"to":9.0,"content":"   "}
        ]}"#;
        let srt = srt_from_json(json).unwrap();
        assert!(srt.starts_with("1\n00:00:01,234 --> 00:00:04,500\n第一句\n"), "{srt}");
        assert!(srt.contains("2\n00:00:04,500 --> 01:01:01,007\n"), "{srt}");
        assert!(srt.contains("第二句\n换行了"), "\\n 要变成真换行: {srt}");
        assert!(!srt.contains("3\n"), "空内容不算一条");
    }

    #[test]
    fn empty_or_broken_input_is_honest() {
        assert_eq!(srt_from_json(r#"{"body":[]}"#).unwrap(), "");
        assert!(srt_from_json("不是 json").is_err());
    }

    #[test]
    fn language_tag_falls_back_to_doc_then_sub() {
        let item = SubtitleItem { lan: "zh-CN".into(), lan_doc: "中文（自动生成）".into(), ..Default::default() };
        assert_eq!(lang_tag(&item), "zh-CN");
        let item = SubtitleItem { lan: String::new(), lan_doc: "英语".into(), ..Default::default() };
        assert_eq!(lang_tag(&item), "sub", "非 ASCII 语言名兜底成 sub");
    }
}
