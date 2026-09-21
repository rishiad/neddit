use utoipa::IntoParams;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct WikiPageQuery {
	pub v: Option<String>,
	pub v2: Option<String>,
}
