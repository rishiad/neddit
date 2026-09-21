use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize, ToSchema)]
#[schema(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum CommentSort {
	#[default]
	Confidence,
	Top,
	New,
	Controversial,
	Old,
	Random,
	Qa,
	Live,
}

impl CommentSort {
	pub(super) const fn as_str(self) -> &'static str {
		match self {
			Self::Confidence => "confidence",
			Self::Top => "top",
			Self::New => "new",
			Self::Controversial => "controversial",
			Self::Old => "old",
			Self::Random => "random",
			Self::Qa => "qa",
			Self::Live => "live",
		}
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, ToSchema)]
#[schema(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum CommentTheme {
	Default,
	Dark,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CommentQuery {
	pub comment: Option<String>,
	pub context: Option<u8>,
	pub depth: Option<u32>,
	pub limit: Option<u32>,
	pub showedits: Option<bool>,
	pub showmedia: Option<bool>,
	pub showmore: Option<bool>,
	pub showtitle: Option<bool>,
	#[param(inline)]
	pub sort: Option<CommentSort>,
	pub sr_detail: Option<bool>,
	#[param(inline)]
	pub theme: Option<CommentTheme>,
	pub threaded: Option<bool>,
	pub truncate: Option<u8>,
}
