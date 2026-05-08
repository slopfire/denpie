use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub enum AppError {
    Db(sqlx::Error),
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
    Json(serde_json::Error),
    Validation(String),
    Auth(String),
    NotFound(String),
    Conflict(String),
    ProtobufDecode(prost::DecodeError),
}

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            AppError::Validation(_) | AppError::ProtobufDecode(_) => StatusCode::BAD_REQUEST,
            AppError::Auth(_) => StatusCode::UNAUTHORIZED,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Db(_) | AppError::Io(_) | AppError::Yaml(_) | AppError::Json(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    pub fn message(&self) -> String {
        match self {
            AppError::Db(err) => err.to_string(),
            AppError::Io(err) => err.to_string(),
            AppError::Yaml(err) => err.to_string(),
            AppError::Json(err) => err.to_string(),
            AppError::Validation(msg)
            | AppError::Auth(msg)
            | AppError::NotFound(msg)
            | AppError::Conflict(msg) => msg.clone(),
            AppError::ProtobufDecode(err) => err.to_string(),
        }
    }

    pub fn into_status_body(self) -> (StatusCode, String) {
        (self.status(), self.message())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        self.into_status_body().into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Db(err)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err)
    }
}

impl From<serde_yaml::Error> for AppError {
    fn from(err: serde_yaml::Error) -> Self {
        AppError::Yaml(err)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Json(err)
    }
}

impl From<prost::DecodeError> for AppError {
    fn from(err: prost::DecodeError) -> Self {
        AppError::ProtobufDecode(err)
    }
}
