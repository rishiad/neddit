use crate::client::error::ClientError;
use crate::service::{ServiceError, ServiceError::Client};
use axum::http::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub(super) enum ApiError {
	#[error(transparent)]
	Service(#[from] ServiceError),
	#[error("invalid `{parameter}` query value `{value}`")]
	InvalidQuery { parameter: String, value: String },
}

impl ApiError {
	pub(super) fn status(&self) -> StatusCode {
		match self {
			Self::InvalidQuery { .. } => StatusCode::BAD_REQUEST,
			Self::Service(error) => service_status(error),
		}
	}

	pub(super) fn message(&self) -> &'static str {
		if matches!(self, Self::Service(ServiceError::ContentBlocked)) {
			return "This server has NSFW disabled";
		}
		match self.status() {
			StatusCode::BAD_REQUEST => "Bad Request",
			StatusCode::UNAUTHORIZED => "Unauthorized",
			StatusCode::FORBIDDEN => "Forbidden",
			StatusCode::NOT_FOUND => "Not Found",
			StatusCode::TOO_MANY_REQUESTS => "Too Many Requests",
			_ => "Bad Gateway",
		}
	}
}

fn service_status(error: &ServiceError) -> StatusCode {
	match error {
		ServiceError::InvalidSubreddit { .. }
		| ServiceError::ConflictingCursors
		| ServiceError::InvalidCursor { .. }
		| ServiceError::InvalidLimit { .. }
		| ServiceError::InvalidParameter { .. } => StatusCode::BAD_REQUEST,
		ServiceError::ContentBlocked => StatusCode::FORBIDDEN,
		ServiceError::Parse(_) | ServiceError::InvalidThreadCommentSearch => StatusCode::BAD_GATEWAY,
		Client(ClientError::Unauthorized) => StatusCode::UNAUTHORIZED,
		Client(ClientError::Quarantined | ClientError::Gated | ClientError::Private | ClientError::Banned | ClientError::Suspended) => StatusCode::FORBIDDEN,
		Client(ClientError::RateLimited { .. }) => StatusCode::TOO_MANY_REQUESTS,
		Client(ClientError::Reddit { code, .. }) => u16::try_from(*code)
			.ok()
			.and_then(|code| StatusCode::from_u16(code).ok())
			.filter(StatusCode::is_client_error)
			.unwrap_or(StatusCode::BAD_GATEWAY),
		Client(_) => StatusCode::BAD_GATEWAY,
	}
}

