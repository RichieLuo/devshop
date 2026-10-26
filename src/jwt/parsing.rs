use super::{AlgorithmChoice, DecodedJwt, JwtError, MAX_TOKEN_BYTES};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;

pub fn decode(input: &str) -> Result<DecodedJwt, JwtError> {
    let token = normalized_token(input)?;
    let parts: Vec<_> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::new(
            "JWT 必须有三段：Header.Payload.Signature（不支持五段 JWE）",
        ));
    }
    if parts.iter().any(|part| part.is_empty()) {
        return Err(JwtError::new(
            "JWT 的 Header、Payload 和 Signature 都不能为空",
        ));
    }
    let header = decode_object(parts[0], "Header")?;
    let payload = decode_object(parts[1], "Payload")?;
    // Validate the third segment even when only displaying a token.
    decode_segment(parts[2], "Signature")?;
    let algorithm = header_algorithm(&header)?;
    if header
        .get("b64")
        .is_some_and(|value| value != &Value::Bool(true))
    {
        return Err(JwtError::new("不支持 b64=false 的未编码 JWS Payload"));
    }
    if let Some(critical) = header.get("crit")
        && critical.as_array().is_none_or(|items| !items.is_empty())
    {
        return Err(JwtError::new(
            "不支持带有 crit 扩展的 JWT，无法保证正确解释关键头字段",
        ));
    }
    Ok(DecodedJwt {
        token: token.to_owned(),
        header_pretty: serde_json::to_string_pretty(&header).expect("JSON value is serializable"),
        payload_pretty: serde_json::to_string_pretty(&payload).expect("JSON value is serializable"),
        header,
        payload,
        algorithm,
        signature: parts[2].to_owned(),
    })
}

fn normalized_token(input: &str) -> Result<&str, JwtError> {
    if input.len() > MAX_TOKEN_BYTES {
        return Err(JwtError::new(
            "JWT 超过 2 MiB 的处理上限，请检查是否粘贴了其他内容",
        ));
    }
    let value = input.trim();
    let value = if value
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
    {
        value[7..].trim()
    } else {
        value
    };
    if value.is_empty() {
        return Err(JwtError::new("请粘贴一个 JWT"));
    }
    Ok(value)
}

fn decode_object(encoded: &str, label: &str) -> Result<Value, JwtError> {
    let bytes = decode_segment(encoded, label)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| JwtError::new(format!("{label} 解码后不是有效 UTF-8 文本")))?;
    parse_object(text, label)
}

fn decode_segment(encoded: &str, label: &str) -> Result<Vec<u8>, JwtError> {
    if !encoded
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(JwtError::new(format!(
            "{label} 不是无填充 Base64URL（不能包含 =、+、/ 或空白）"
        )));
    }
    URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| JwtError::new(format!("{label} 的 Base64URL 编码无效")))
}

pub(crate) fn parse_object(text: &str, label: &str) -> Result<Value, JwtError> {
    if text.len() > MAX_TOKEN_BYTES {
        return Err(JwtError::new(format!("{label} 超过 2 MiB 的处理上限")));
    }
    let mut deserializer = serde_json::Deserializer::from_str(text);
    UniqueKeys::deserialize(&mut deserializer)
        .and_then(|_| deserializer.end())
        .map_err(|error| JwtError::new(format!("{label} JSON 无效：{error}")))?;
    let value: Value = serde_json::from_str(text)
        .map_err(|error| JwtError::new(format!("{label} JSON 无效：{error}")))?;
    if !value.is_object() {
        return Err(JwtError::new(format!("{label} 必须是 JSON 对象")));
    }
    Ok(value)
}

pub(crate) fn header_algorithm(header: &Value) -> Result<AlgorithmChoice, JwtError> {
    let name = header
        .get("alg")
        .and_then(Value::as_str)
        .ok_or_else(|| JwtError::new("Header 缺少字符串类型的 alg 字段"))?;
    AlgorithmChoice::parse(name)
}

// Inspect every object before deserializing Value, which otherwise accepts the
// last duplicate key. Ambiguous alg / exp / nested claims must not be hidden.
struct UniqueKeys;

impl<'de> Deserialize<'de> for UniqueKeys {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueKeys;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("JSON with unique object keys")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut keys = HashSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !keys.insert(key.clone()) {
                        return Err(serde::de::Error::custom(format!("重复字段 {key:?}")));
                    }
                    map.next_value::<UniqueKeys>()?;
                }
                Ok(UniqueKeys)
            }
            fn visit_seq<S: SeqAccess<'de>>(
                self,
                mut sequence: S,
            ) -> Result<Self::Value, S::Error> {
                while sequence.next_element::<UniqueKeys>()?.is_some() {}
                Ok(UniqueKeys)
            }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(UniqueKeys)
            }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
                Ok(UniqueKeys)
            }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
                Ok(UniqueKeys)
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(UniqueKeys)
            }
            fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
                Ok(UniqueKeys)
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueKeys)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueKeys)
            }
            fn visit_some<D: serde::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Self::Value, D::Error> {
                IgnoredAny::deserialize(deserializer).map(|_| UniqueKeys)
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}
