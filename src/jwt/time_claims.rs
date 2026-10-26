use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeStatus {
    Expired,
    Active,
    NotYetValid,
    IssuedInFuture,
    Informational,
    Invalid,
}

impl TimeStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Expired => "已过期",
            Self::Active => "时间条件满足",
            Self::NotYetValid => "尚未生效",
            Self::IssuedInFuture => "签发时间在未来",
            Self::Informational => "签发时间",
            Self::Invalid => "无效时间字段",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClaimTime {
    pub name: &'static str,
    pub value: String,
    pub utc: String,
    pub status: TimeStatus,
}

/// Display registered NumericDate claims at a supplied clock instant. This is
/// diagnostic information, not an authentication or authorization decision.
pub fn claim_times(payload: &Value, now_seconds: i64) -> Vec<ClaimTime> {
    ["exp", "nbf", "iat"]
        .into_iter()
        .filter_map(|name| {
            let value = payload.get(name)?;
            let Some((seconds, nanos)) = numeric_date(value) else {
                return Some(ClaimTime {
                    name,
                    value: value.to_string(),
                    utc: "应为秒级 NumericDate 数值".into(),
                    status: TimeStatus::Invalid,
                });
            };
            let Some(date) = DateTime::<Utc>::from_timestamp(seconds, nanos) else {
                return Some(ClaimTime {
                    name,
                    value: value.to_string(),
                    utc: "超出可显示的日期范围".into(),
                    status: TimeStatus::Invalid,
                });
            };
            let in_future = seconds > now_seconds || (seconds == now_seconds && nanos > 0);
            let status = match name {
                "exp" if !in_future => TimeStatus::Expired,
                "nbf" if in_future => TimeStatus::NotYetValid,
                "iat" if in_future => TimeStatus::IssuedInFuture,
                "iat" => TimeStatus::Informational,
                _ => TimeStatus::Active,
            };
            Some(ClaimTime {
                name,
                value: value.to_string(),
                utc: date.to_rfc3339(),
                status,
            })
        })
        .collect()
}

fn numeric_date(value: &Value) -> Option<(i64, u32)> {
    if let Some(seconds) = value.as_i64() {
        return Some((seconds, 0));
    }
    let number = value.as_f64()?;
    if !number.is_finite() || number < i64::MIN as f64 || number >= i64::MAX as f64 {
        return None;
    }
    let floor = number.floor();
    Some((
        floor as i64,
        ((number - floor) * 1_000_000_000.0)
            .floor()
            .clamp(0.0, 999_999_999.0) as u32,
    ))
}
