//! Local JWT inspection. Decoding, cryptographic verification and claim timing
//! are deliberately separate operations: decoded data is not authenticated.

mod crypto;
mod parsing;
mod time_claims;

pub use crypto::{sign, verify_signature};
pub use parsing::decode;
pub use time_claims::{ClaimTime, TimeStatus, claim_times};

use std::fmt;

pub const MAX_TOKEN_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlgorithmChoice {
    #[default]
    Hs256,
    Hs384,
    Hs512,
    Rs256,
    Rs384,
    Rs512,
    Ps256,
    Ps384,
    Ps512,
    Es256,
    Es384,
    EdDsa,
}

impl AlgorithmChoice {
    pub const ALL: [Self; 12] = [
        Self::Hs256,
        Self::Hs384,
        Self::Hs512,
        Self::Rs256,
        Self::Rs384,
        Self::Rs512,
        Self::Ps256,
        Self::Ps384,
        Self::Ps512,
        Self::Es256,
        Self::Es384,
        Self::EdDsa,
    ];

    pub fn parse(value: &str) -> Result<Self, JwtError> {
        Self::ALL
            .into_iter()
            .find(|algorithm| algorithm.name() == value)
            .ok_or_else(|| {
                JwtError::new(if value.eq_ignore_ascii_case("none") {
                    "拒绝 alg=none：不支持无签名令牌".to_owned()
                } else {
                    format!("暂不支持签名算法 {value}；支持 HS / RS / PS / ES 和 EdDSA")
                })
            })
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Hs256 => "HS256",
            Self::Hs384 => "HS384",
            Self::Hs512 => "HS512",
            Self::Rs256 => "RS256",
            Self::Rs384 => "RS384",
            Self::Rs512 => "RS512",
            Self::Ps256 => "PS256",
            Self::Ps384 => "PS384",
            Self::Ps512 => "PS512",
            Self::Es256 => "ES256",
            Self::Es384 => "ES384",
            Self::EdDsa => "EdDSA",
        }
    }

    pub const fn is_hmac(self) -> bool {
        matches!(self, Self::Hs256 | Self::Hs384 | Self::Hs512)
    }

    pub(crate) const fn native(self) -> jsonwebtoken::Algorithm {
        use jsonwebtoken::Algorithm;
        match self {
            Self::Hs256 => Algorithm::HS256,
            Self::Hs384 => Algorithm::HS384,
            Self::Hs512 => Algorithm::HS512,
            Self::Rs256 => Algorithm::RS256,
            Self::Rs384 => Algorithm::RS384,
            Self::Rs512 => Algorithm::RS512,
            Self::Ps256 => Algorithm::PS256,
            Self::Ps384 => Algorithm::PS384,
            Self::Ps512 => Algorithm::PS512,
            Self::Es256 => Algorithm::ES256,
            Self::Es384 => Algorithm::ES384,
            Self::EdDsa => Algorithm::EdDSA,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyEncoding {
    #[default]
    Text,
    Base64,
}

#[derive(Clone, Debug)]
pub struct DecodedJwt {
    pub token: String,
    pub header: serde_json::Value,
    pub payload: serde_json::Value,
    pub algorithm: AlgorithmChoice,
    pub header_pretty: String,
    pub payload_pretty: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JwtError {
    pub message: String,
}

impl JwtError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for JwtError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JwtError {}
