const PREFIX: &str = "EGGDONE_ERROR";

pub const SYNC_CREDENTIALS: &str = "SYNC_CREDENTIALS";
pub const SYNC_NETWORK: &str = "SYNC_NETWORK";
pub const SYNC_FAILED: &str = "SYNC_FAILED";
pub const REMINDER_FAILED: &str = "REMINDER_FAILED";
pub const FOCUS_UNAVAILABLE: &str = "FOCUS_UNAVAILABLE";

pub fn coded(code: &str, detail: impl AsRef<str>) -> String {
    let detail = detail
        .as_ref()
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(400)
        .collect::<String>();
    format!("{PREFIX}::{code}::{detail}")
}

pub fn sync(detail: String) -> String {
    let normalized = detail.to_ascii_lowercase();
    let code = if normalized.contains("credential")
        || normalized.contains("access key")
        || normalized.contains("secret key")
        || detail.contains("凭据")
    {
        SYNC_CREDENTIALS
    } else if normalized.contains("network")
        || normalized.contains("timeout")
        || normalized.contains("connection")
        || detail.contains("网络")
        || detail.contains("连接")
    {
        SYNC_NETWORK
    } else {
        SYNC_FAILED
    };
    coded(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_sync_errors_without_exposing_multiline_stacks() {
        assert!(sync("network timeout\nstack".to_string())
            .starts_with("EGGDONE_ERROR::SYNC_NETWORK::network timeout"));
        assert_eq!(
            coded(FOCUS_UNAVAILABLE, "missing"),
            "EGGDONE_ERROR::FOCUS_UNAVAILABLE::missing"
        );
    }
}
