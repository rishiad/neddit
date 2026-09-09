use axum::{
	http::StatusCode,
	response::{IntoResponse, Response},
};
use neddit_api::service::ServiceError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
	#[error("invalid feed sort")]
	InvalidSort,
	#[error("invalid comment sort")]
	InvalidCommentSort,
	#[error("post not found")]
	PostNotFound,
	#[error("failed to load the Reddit feed")]
	Service(#[from] ServiceError),
}

impl IntoResponse for AppError {
	fn into_response(self) -> Response {
		let status = match self {
			Self::InvalidSort | Self::InvalidCommentSort => StatusCode::BAD_REQUEST,
			Self::PostNotFound => StatusCode::NOT_FOUND,
			Self::Service(_) => StatusCode::BAD_GATEWAY,
		};
		(status, self.to_string()).into_response()
	}
}
