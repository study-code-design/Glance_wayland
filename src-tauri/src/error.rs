use tauri::ipc::InvokeError;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("api error: {0}")]
    Api(String),
    #[error("base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("capture error: {0}")]
    Capture(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("mime error: {0}")]
    Mime(reqwest::Error),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("tauri error: {0}")]
    Tauri(#[from] tauri::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
}

impl From<AppError> for InvokeError {
    fn from(value: AppError) -> Self {
        InvokeError::from(value.to_string())
    }
}
