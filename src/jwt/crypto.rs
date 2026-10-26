use super::parsing::{header_algorithm, parse_object};
use super::{AlgorithmChoice, JwtError, KeyEncoding, decode};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use jsonwebtoken::{DecodingKey, EncodingKey};

/// Verify only the cryptographic signature, without conflating it with exp,
/// nbf, issuer or audience policy. The caller chooses the expected algorithm.
pub fn verify_signature(
    token: &str,
    expected: AlgorithmChoice,
    key: &str,
    encoding: KeyEncoding,
) -> Result<bool, JwtError> {
    let decoded = decode(token)?;
    require_algorithm(decoded.algorithm, expected)?;
    let decoding_key = decoding_key(expected, key, encoding)?;
    let (message, signature) = decoded
        .token
        .rsplit_once('.')
        .expect("decoded JWT has three parts");
    jsonwebtoken::crypto::verify(
        signature,
        message.as_bytes(),
        &decoding_key,
        expected.native(),
    )
    .map_err(|error| JwtError::new(format!("验签失败：{error}")))
}

pub fn sign(
    header_json: &str,
    payload_json: &str,
    expected: AlgorithmChoice,
    key: &str,
    encoding: KeyEncoding,
) -> Result<String, JwtError> {
    let header = parse_object(header_json, "Header")?;
    let payload = parse_object(payload_json, "Payload")?;
    require_algorithm(header_algorithm(&header)?, expected)?;
    let header = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).expect("serializable JSON"));
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).expect("serializable JSON"));
    let message = format!("{header}.{payload}");
    let key = encoding_key(expected, key, encoding)?;
    let signature = jsonwebtoken::crypto::sign(message.as_bytes(), &key, expected.native())
        .map_err(|error| JwtError::new(format!("签名失败：{error}")))?;
    let token = format!("{message}.{signature}");
    // Apply the same critical-extension, size and shape rules to generated JWTs.
    decode(&token)?;
    Ok(token)
}

fn require_algorithm(actual: AlgorithmChoice, expected: AlgorithmChoice) -> Result<(), JwtError> {
    if actual != expected {
        return Err(JwtError::new(format!(
            "算法不匹配：Header 为 {}，当前明确选择的是 {}",
            actual.name(),
            expected.name()
        )));
    }
    Ok(())
}

fn hmac_secret(key: &str, encoding: KeyEncoding) -> Result<Vec<u8>, JwtError> {
    if key.is_empty() {
        return Err(JwtError::new("请输入共享密钥，空密钥不参与签名或验签"));
    }
    if key.trim_start().starts_with("-----BEGIN ") {
        return Err(JwtError::new(
            "HMAC 算法不接受 PEM 公钥或私钥，请确认所选算法",
        ));
    }
    match encoding {
        KeyEncoding::Text => Ok(key.as_bytes().to_vec()),
        KeyEncoding::Base64 => {
            let bytes = STANDARD
                .decode(key.trim())
                .or_else(|_| STANDARD_NO_PAD.decode(key.trim()))
                .map_err(|_| JwtError::new("共享密钥不是有效的标准 Base64"))?;
            if bytes.is_empty() {
                return Err(JwtError::new("Base64 解码后的共享密钥不能为空"));
            }
            Ok(bytes)
        }
    }
}

fn decoding_key(
    algorithm: AlgorithmChoice,
    key: &str,
    encoding: KeyEncoding,
) -> Result<DecodingKey, JwtError> {
    if algorithm.is_hmac() {
        return Ok(DecodingKey::from_secret(&hmac_secret(key, encoding)?));
    }
    let result = match algorithm {
        AlgorithmChoice::Rs256
        | AlgorithmChoice::Rs384
        | AlgorithmChoice::Rs512
        | AlgorithmChoice::Ps256
        | AlgorithmChoice::Ps384
        | AlgorithmChoice::Ps512 => DecodingKey::from_rsa_pem(key.as_bytes()),
        AlgorithmChoice::Es256 | AlgorithmChoice::Es384 => DecodingKey::from_ec_pem(key.as_bytes()),
        AlgorithmChoice::EdDsa => DecodingKey::from_ed_pem(key.as_bytes()),
        _ => unreachable!(),
    };
    result.map_err(|error| JwtError::new(format!("公钥 PEM 无效：{error}")))
}

fn encoding_key(
    algorithm: AlgorithmChoice,
    key: &str,
    encoding: KeyEncoding,
) -> Result<EncodingKey, JwtError> {
    if algorithm.is_hmac() {
        return Ok(EncodingKey::from_secret(&hmac_secret(key, encoding)?));
    }
    let result = match algorithm {
        AlgorithmChoice::Rs256
        | AlgorithmChoice::Rs384
        | AlgorithmChoice::Rs512
        | AlgorithmChoice::Ps256
        | AlgorithmChoice::Ps384
        | AlgorithmChoice::Ps512 => EncodingKey::from_rsa_pem(key.as_bytes()),
        AlgorithmChoice::Es256 | AlgorithmChoice::Es384 => EncodingKey::from_ec_pem(key.as_bytes()),
        AlgorithmChoice::EdDsa => EncodingKey::from_ed_pem(key.as_bytes()),
        _ => unreachable!(),
    };
    result.map_err(|error| JwtError::new(format!("签名需要有效私钥 PEM：{error}")))
}
