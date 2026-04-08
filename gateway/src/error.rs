use serde::Serialize;
use thiserror::Error;

#[derive(Serialize)]
pub(crate) struct ErrorResponse<'a> {
    pub error: &'a str,
    pub message: &'a str,
    pub status: u16,
}

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error(transparent)]
    ConfigLoad(#[from] config::ConfigError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] http::Error),

    #[error("invalid URI: {0}")]
    InvalidUri(#[from] http::uri::InvalidUri),

    #[error("proxy error: {0}")]
    Proxy(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
