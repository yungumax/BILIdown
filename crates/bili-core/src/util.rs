//! 通用小工具。

/// 清理文件名：替换 Windows 非法字符、去掉首尾空白与结尾的点，并限制长度。
pub fn sanitize_filename(name: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_windows_reserved_characters() {
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
        assert_eq!(sanitize_filename("标题..."), "标题");
    }
}
