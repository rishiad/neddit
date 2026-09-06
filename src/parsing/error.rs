use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
	#[error("invalid {entity} payload")]
	InvalidPayload {
		entity: &'static str,
		#[source]
		source: serde_json::Error,
	},

	#[error("invalid `{field}` field in {entity}")]
	InvalidField { entity: &'static str, field: &'static str },

	#[error("unsupported {entity} kind `{kind}`")]
	UnsupportedKind { entity: &'static str, kind: String },
}
