//! B 站 wbi 签名实现。
//!
//! 流程：从 `nav` 接口拿到 img_key / sub_key → 用固定混淆表重排出 mixin_key
//! → 请求参数按 key 排序后拼成 query（附加时间戳 wts）→ MD5(query + mixin_key) 得到 w_rid。
//! 最终签名串必须与真正发出去的 query 完全一致，因此这里直接产出完整 query 字符串。

use md5::{Digest, Md5};
use std::time::{SystemTime, UNIX_EPOCH};

/// 官方固定混淆表：把 img_key+sub_key 拼接后的 64 位字符按此顺序重排，取前 32 位。
const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

/// 一组 wbi 密钥，来自 `nav` 接口的 `wbi_img`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WbiKeys {
    pub img_key: String,
    pub sub_key: String,
}

impl WbiKeys {
    /// 生成带签名的完整 query 字符串（含 `wts` 与 `w_rid`）。
    pub fn sign_query<'a, I>(&self, params: I) -> String
    where
        I: IntoIterator<Item = (&'a str, String)>,
    {
        self.sign_query_with_wts(params, now_secs())
    }

    /// 指定时间戳的签名，便于用官方文档示例做回归测试。
    pub fn sign_query_with_wts<'a, I>(&self, params: I, wts: u64) -> String
    where
        I: IntoIterator<Item = (&'a str, String)>,
    {
        let mut sorted: Vec<(String, String)> = params
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        sorted.push(("wts".to_string(), wts.to_string()));
        sorted.sort_by(|a, b| a.0.cmp(&b.0));

        let query = sorted
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    encode_component(k),
                    encode_component(&filter_value(v))
                )
            })
            .collect::<Vec<_>>()
            .join("&");

        let mixin = mixin_key(&self.img_key, &self.sub_key);
        let mut hasher = Md5::new();
        hasher.update(query.as_bytes());
        hasher.update(mixin.as_bytes());
        let w_rid = hex::encode(hasher.finalize());

        format!("{query}&w_rid={w_rid}")
    }
}

/// 由 img_key 与 sub_key 计算 mixin_key（取重排结果的前 32 位）。
pub fn mixin_key(img_key: &str, sub_key: &str) -> String {
    let raw: Vec<char> = format!("{img_key}{sub_key}").chars().collect();
    MIXIN_KEY_ENC_TAB
        .iter()
        .filter_map(|&i| raw.get(i).copied())
        .take(32)
        .collect()
}

/// 从 wbi_img 的图片 URL 中取出密钥（去掉目录与扩展名）。
pub fn key_from_url(url: &str) -> String {
    url.rsplit('/')
        .next()
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("")
        .to_string()
}

/// wbi 规范要求过滤掉这些字符，避免与浏览器端编码结果不一致。
fn filter_value(value: &str) -> String {
    value.chars().filter(|c| !"!'()*".contains(*c)).collect()
}

/// 与 JS `encodeURIComponent` 一致的百分号编码（空格编码为 %20 而非 +）。
fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.as_bytes() {
        let c = *byte as char;
        let unreserved = c.is_ascii_alphanumeric()
            || matches!(c, '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')');
        if unreserved {
            out.push(c);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixin_key_has_expected_shape() {
        let key = mixin_key(
            "7cd084941338484aae1ad9425b84077c",
            "4932caff0ff746eab6f01bf08b70ac45",
        );
        assert_eq!(key.len(), 32, "mixin_key 长度必须为 32");
        assert!(
            key.chars().all(|c| c.is_ascii_hexdigit()),
            "mixin_key 应为十六进制字符"
        );
    }

    #[test]
    fn mixin_key_is_deterministic_and_order_sensitive() {
        let a = mixin_key("aaaa", "bbbb");
        let b = mixin_key("aaaa", "bbbb");
        let c = mixin_key("bbbb", "aaaa");
        assert_eq!(a, b);
        assert_ne!(a, c, "交换 img/sub key 应得到不同的 mixin_key");
    }

    #[test]
    fn key_from_url_strips_path_and_extension() {
        assert_eq!(
            key_from_url("https://i0.hdslb.com/bfs/wbi/7cd084941338484aae1ad9425b84077c.png"),
            "7cd084941338484aae1ad9425b84077c"
        );
    }

    #[test]
    fn sign_query_is_sorted_and_carries_signature() {
        let keys = WbiKeys {
            img_key: "7cd084941338484aae1ad9425b84077c".into(),
            sub_key: "4932caff0ff746eab6f01bf08b70ac45".into(),
        };
        let query = keys.sign_query(vec![
            ("zab", "1919810".to_string()),
            ("foo", "114".to_string()),
            ("bar", "514".to_string()),
        ]);

        // 参数按 key 升序排列，且 wts/w_rid 就位
        let order: Vec<&str> = query
            .split('&')
            .filter_map(|kv| kv.split('=').next())
            .collect();
        assert_eq!(order, vec!["bar", "foo", "wts", "zab", "w_rid"]);
        assert!(query.contains("w_rid="));

        // 签名一定是 32 位十六进制
        let w_rid = query.rsplit("w_rid=").next().unwrap();
        assert_eq!(w_rid.len(), 32);
        assert!(w_rid.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sign_query_filters_specials_and_encodes_unicode() {
        let keys = WbiKeys {
            img_key: "7cd084941338484aae1ad9425b84077c".into(),
            sub_key: "4932caff0ff746eab6f01bf08b70ac45".into(),
        };
        let query = keys.sign_query(vec![("keyword", "测试!'()* 值".to_string())]);
        assert!(
            query.contains("keyword=%E6%B5%8B%E8%AF%95%20%E5%80%BC"),
            "实际: {query}"
        );
    }

    /// 回归测试：与 bilibili-api-collect 文档公布的示例值逐字比对。
    #[test]
    fn matches_documented_reference_vector() {
        let keys = WbiKeys {
            img_key: "7cd084941338484aae1ad9425b84077c".into(),
            sub_key: "4932caff0ff746eab6f01bf08b70ac45".into(),
        };
        assert_eq!(
            mixin_key(&keys.img_key, &keys.sub_key),
            "ea1db124af3c7062474693fa704f4ff8"
        );
        let query = keys.sign_query_with_wts(
            vec![
                ("foo", "114".to_string()),
                ("bar", "514".to_string()),
                ("zab", "1919810".to_string()),
            ],
            1702204169,
        );
        assert_eq!(
            query,
            "bar=514&foo=114&wts=1702204169&zab=1919810&w_rid=8f6f2b5b3d485fe1886cec6a0be8c5d4"
        );
    }
}
