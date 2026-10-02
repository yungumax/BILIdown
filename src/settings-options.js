/**
 * 媒体选项的单一来源：设置页与首次启动引导页共用，避免两处标签漂移。
 */

// 优先顺序列表里的画质项：带上档位号，和 B 站文档里的 qn 对得上
export const QUALITY_PREFS = [
  { value: 127, label: "8K / 127" },
  { value: 126, label: "杜比视界 / 126" },
  { value: 125, label: "HDR / 125" },
  { value: 120, label: "4K / 120" },
  { value: 116, label: "1080P60 / 116" },
  { value: 112, label: "1080P+ / 112" },
  { value: 80, label: "1080P / 80" },
  { value: 74, label: "720P60 / 74" },
  { value: 64, label: "720P / 64" },
  { value: 32, label: "480P / 32" },
  { value: 16, label: "360P / 16" },
];

// 「视频清晰度」下拉：0 = 最优画质（不高于任何档 → 取可用最高），
// 其余选项复用优先顺序表里的档位（带 qn，好对文档）
export const QUALITY_CHOICES = [{ value: 0, label: "最优画质" }, ...QUALITY_PREFS];

export const AUDIO_PREFS = [
  { value: "auto", label: "最佳可用" },
  { value: "flac", label: "Hi-Res 无损" },
  { value: "dolby", label: "杜比全景声" },
  { value: "normal", label: "普通音轨" },
];

// 视频格式（封装）：MKV 已去掉——封面、字幕、弹幕都成独立文件后它没用了
export const VIDEO_FORMATS = [
  { value: "mp4", label: "MP4（通用）" },
  { value: "ts", label: "TS（剪辑 / 直播工具友好）" },
];

// 音频格式：只影响「音频」来源的成品，视频里的音轨保持原编码
export const AUDIO_FORMATS = [
  { value: "source", label: "原格式（不转码）" },
  { value: "mp3", label: "MP3（兼容最广）" },
];

// 图片格式：只影响图文图片与封面
export const IMAGE_FORMATS = [
  { value: "source", label: "原格式（不转码）" },
  { value: "jpg", label: "JPG（体积小）" },
];
