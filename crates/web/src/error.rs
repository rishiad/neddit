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
	#[error("invalid feed time")]
	InvalidFeedTime,
	#[error("invalid comment sort")]
	InvalidCommentSort,
	#[error("comment search must contain at most 512 characters")]
	InvalidCommentSearch,
	#[error("post not found")]
	PostNotFound,
	#[error("failed to load Reddit data")]
	Service(#[from] ServiceError),
}

impl IntoResponse for AppError {
	fn into_response(self) -> Response {
		let status = match self {
			Self::InvalidSort | Self::InvalidFeedTime | Self::InvalidCommentSort | Self::InvalidCommentSearch => StatusCode::BAD_REQUEST,
			Self::PostNotFound => StatusCode::NOT_FOUND,
			Self::Service(_) => StatusCode::BAD_GATEWAY,
		};
		(status, self.to_string()).into_response()
	}
}
