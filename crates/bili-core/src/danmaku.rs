//! 弹幕：把 B 站的弹幕接口转成播放器能直接读的 XML。
//!
//! 两条路：
//!
//! - `seg.so`（protobuf，主路）：网页播放器用的就是这个，按 6 分钟一段分页，
//!   翻完所有段就是**全量**弹幕。这里手写一个够用的 protobuf 读取器，
//!   不引 protobuf 依赖——只认我们需要的那几个字段，其余按 wire type 跳过。
//! - `comment.bilibili.com/{cid}.xml`（兜底）：明文 XML，但**只给一小部分**
//!   （实测 91 万弹幕的视频只返回 1200 条），所以只在 protobuf 那条路失败时用。
//!
//! 输出的 `<d p="...">` 就是 B 站自己的格式：进度(秒,带小数),模式,字号,颜色,
//! 时间戳,弹幕池,用户 hash,行号 id。弹弹play / mpv 之类直接认。

/// 一条弹幕。字段名按 B 站 protobuf 的定义来，只留写进 XML 需要的。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DanmakuElem {
    pub id: i64,
    /// 出现时间（毫秒）
    pub progress_ms: i32,
    /// 1 滚动 / 4 底部 / 5 顶部 …
    pub mode: i32,
    pub fontsize: i32,
    pub color: u32,
    /// 发送时间（秒）
    pub ctime: i64,
    pub mid_hash: String,
    pub content: String,
}

/// 解一段 `seg.so` 的响应。空 body（翻过了最后一页）返回空表。
pub fn parse_segment(bytes: &[u8]) -> Vec<DanmakuElem> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some((key, next)) = read_varint(bytes, cursor) {
        cursor = next;
        let field = key >> 3;
        let wire = key & 0x07;
        match wire {
            // 顶层 field 1 = 一条弹幕
            2 => {
                let Some((len, next)) = read_varint(bytes, cursor) else { break };
                cursor = next;
                let end = (cursor + len as usize).min(bytes.len());
                if field == 1 {
                    if let Some(elem) = parse_elem(&bytes[cursor..end]) {
                        out.push(elem);
                    }
                }
                cursor = end;
            }
            0 => {
                let Some((_, next)) = read_varint(bytes, cursor) else { break };
                cursor = next;
            }
            1 => cursor += 8,
            5 => cursor += 4,
            _ => break,
        }
        if cursor >= bytes.len() {
            break;
        }
    }
    out
}

fn parse_elem(buf: &[u8]) -> Option<DanmakuElem> {
    let mut elem = DanmakuElem::default();
    let mut cursor = 0usize;
    while let Some((key, next)) = read_varint(buf, cursor) {
        cursor = next;
        let field = key >> 3;
        let wire = key & 0x07;
        match wire {
            0 => {
                let (value, next) = read_varint(buf, cursor)?;
                cursor = next;
                match field {
                    1 => elem.id = value as i64,
                    2 => elem.progress_ms = value as i32,
                    3 => elem.mode = value as i32,
                    4 => elem.fontsize = value as i32,
                    5 => elem.color = value as u32,
                    8 => elem.ctime = value as i64,
                    _ => {}
                }
            }
            2 => {
                let (len, next) = read_varint(buf, cursor)?;
                cursor = next;
                let end = (cursor + len as usize).min(buf.len());
                let text = String::from_utf8_lossy(&buf[cursor..end]).to_string();
                if field == 6 {
                    elem.mid_hash = text;
                } else if field == 7 {
                    elem.content = text;
                }
                cursor = end;
            }
            1 => cursor += 8,
            5 => cursor += 4,
            _ => return None,
        }
        if cursor >= buf.len() {
            break;
        }
    }
    if elem.content.is_empty() && elem.progress_ms == 0 {
        return None;
    }
    Some(elem)
}

fn read_varint(buf: &[u8], mut cursor: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = *buf.get(cursor)?;
        cursor += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((value, cursor));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
}

/// 把一批弹幕拼成 B 站格式的 XML（播放器直接读）。
pub fn to_xml(elems: &[DanmakuElem]) -> String {
    let mut sorted: Vec<&DanmakuElem> = elems.iter().collect();
    sorted.sort_by_key(|e| e.progress_ms);
    let mut xml = String::with_capacity(sorted.len() * 96 + 256);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<i>\n");
    xml.push_str("  <chatserver>chat.bilibili.com</chatserver>\n");
    xml.push_str("  <chatid>");
    xml.push_str(&sorted.first().map(|e| e.id).unwrap_or(0).to_string());
    xml.push_str("</chatid>\n");
    for elem in sorted {
        xml.push_str("  <d p=\"");
        // 进度按秒给，五位小数（和 B 站自己的文件一致）
        xml.push_str(&format!("{:.5}", elem.progress_ms as f64 / 1000.0));
        xml.push_str(&format!(
            ",{},{},{},{},0,{},{}",
            elem.mode,
            elem.fontsize.max(12),
            elem.color,
            elem.ctime,
            elem.mid_hash,
            elem.id
        ));
        xml.push_str("\">");
        xml.push_str(&escape_xml(&elem.content));
        xml.push_str("</d>\n");
    }
    xml.push_str("</i>\n");
    xml
}

fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 不允许的字符（部分弹幕里真有）直接丢掉，别写出坏文件
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint(mut value: u64, out: &mut Vec<u8>) {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                break;
            }
        }
    }

    fn field_varint(field: u64, value: u64, out: &mut Vec<u8>) {
        varint(field << 3, out);
        varint(value, out);
    }

    fn field_bytes(field: u64, text: &str, out: &mut Vec<u8>) {
        varint((field << 3) | 2, out);
        varint(text.len() as u64, out);
        out.extend_from_slice(text.as_bytes());
    }

    fn one_elem(id: u64, progress: u64, mode: u64, color: u64, ctime: u64, hash: &str, content: &str) -> Vec<u8> {
        let mut elem = Vec::new();
        field_varint(1, id, &mut elem);
        field_varint(2, progress, &mut elem);
        field_varint(3, mode, &mut elem);
        field_varint(4, 25, &mut elem);
        field_varint(5, color, &mut elem);
        field_bytes(6, hash, &mut elem);
        field_bytes(7, content, &mut elem);
        field_varint(8, ctime, &mut elem);
        let mut seg = Vec::new();
        varint((1 << 3) | 2, &mut seg);
        varint(elem.len() as u64, &mut seg);
        seg.extend_from_slice(&elem);
        seg
    }

    #[test]
    fn parses_segment_with_unknown_fields_skipped() {
        let mut seg = one_elem(59044123, 52400, 1, 16777215, 1320891024, "f4dbdf21", "很有观赏性");
        // 未知字段（field 26 varint、field 20 字符串）不该把解析带偏
        let mut tail = Vec::new();
        field_varint(26, 279786, &mut tail);
        field_bytes(20, "0", &mut tail);
        seg.extend_from_slice(&tail);

        let elems = parse_segment(&seg);
        assert_eq!(elems.len(), 1);
        assert_eq!(
            elems[0],
            DanmakuElem {
                id: 59044123,
                progress_ms: 52400,
                mode: 1,
                fontsize: 25,
                color: 16777215,
                ctime: 1320891024,
                mid_hash: "f4dbdf21".to_string(),
                content: "很有观赏性".to_string(),
            }
        );
        assert!(parse_segment(&[]).is_empty(), "翻过最后一页是空 body");
    }

    #[test]
    fn xml_matches_bilibili_shape_and_escapes() {
        let elems = vec![
            DanmakuElem {
                id: 59044123,
                progress_ms: 23826,
                mode: 1,
                fontsize: 25,
                color: 16777215,
                ctime: 1320891024,
                mid_hash: "f4dbdf21".to_string(),
                content: "阵亡.<b>&\"引号\"".to_string(),
            },
            DanmakuElem {
                id: 2,
                progress_ms: 1000,
                mode: 5,
                fontsize: 18,
                color: 255,
                ctime: 1320891000,
                mid_hash: "abcd".to_string(),
                content: "早".to_string(),
            },
        ];
        let xml = to_xml(&elems);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<i>"));
        assert!(xml.trim_end().ends_with("</i>"));
        // 排序：1.0 秒那条在前
        let first = xml.find("<d p=").unwrap();
        assert!(xml[first..].starts_with("<d p=\"1.00000,5,18,255,1320891000,0,abcd,2\">早</d>"));
        assert!(xml.contains("<d p=\"23.82600,1,25,16777215,1320891024,0,f4dbdf21,59044123\">"));
        assert!(xml.contains("阵亡.&lt;b&gt;&amp;&quot;引号&quot;"), "内容里的 XML 元字符要转义");
        assert_eq!(xml.matches("<d p=").count(), 2);
    }
}
