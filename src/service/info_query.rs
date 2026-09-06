use utoipa::IntoParams;

#[derive(Clone, Debug, Default, Eq, PartialEq, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InfoQuery {
	#[param(rename = "id", explode = false, required = false)]
	pub ids: Vec<String>,
	#[param(rename = "sr_name", explode = false, required = false)]
	pub subreddit_names: Vec<String>,
	pub url: Option<String>,
}
