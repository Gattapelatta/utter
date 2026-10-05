use std::time::Duration;

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    #[error("API key rejected")]
    Unauthorized,
    #[error("rate limited")]
    RateLimited { retry_after: Option<Duration> },
    #[error("network unavailable")]
    Network,
    #[error("request timed out")]
    Timeout,
    #[error("server error {0}")]
    Server(u16),
    #[error("unexpected response: {0}")]
    BadResponse(String),
}
