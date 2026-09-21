use utoipa::IntoParams;

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InfoQuery {
	#[param(rename = "id", explode = false, required = false)]
	#[serde(rename = "id", default, with = "crate::service::query_codec::comma", skip_serializing_if = "Vec::is_empty")]
	pub ids: Vec<String>,
	#[param(rename = "sr_name", explode = false, required = false)]
	#[serde(rename = "sr_name", default, with = "crate::service::query_codec::comma", skip_serializing_if = "Vec::is_empty")]
	pub subreddit_names: Vec<String>,
	pub url: Option<String>,
}
