use crate::api::error::ApiError;
use axum::{
	body::Body,
	http::{header, HeaderValue, StatusCode},
	response::Response,
};
use log::error;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub(super) struct ErrorBody {
	message: &'static str,
	error: u16,
}

pub(super) fn respond<T>(result: Result<T, ApiError>) -> Response
where
	T: Serialize,
{
	match result {
		Ok(value) => json(StatusCode::OK, &value),
		Err(api_error) => {
			let status = api_error.status();
			if status.is_server_error() {
				error!("Reddit API route failed: {api_error:?}");
			}
			json(
				status,
				&ErrorBody {
					message: api_error.message(),
					error: status.as_u16(),
				},
			)
		}
	}
}

fn json<T>(status: StatusCode, value: &T) -> Response
where
	T: Serialize,
{
	let (status, body) = match serde_json::to_vec(value) {
		Ok(body) => (status, body),
		Err(error) => {
			error!("failed to serialize Reddit API response: {error}");
			(StatusCode::INTERNAL_SERVER_ERROR, br#"{"message":"Internal Server Error","error":500}"#.to_vec())
		}
	};
	let mut response = Response::new(Body::from(body));
	*response.status_mut() = status;
	response
		.headers_mut()
		.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json; charset=utf-8"));
	response
}
