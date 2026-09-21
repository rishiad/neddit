use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, ToSchema)]
#[schema(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ListingShow {
	All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ToSchema)]
#[schema(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ListingTime {
	Hour,
	Day,
	Week,
	Month,
	Year,
	All,
}

impl ListingTime {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Hour => "hour",
			Self::Day => "day",
			Self::Week => "week",
			Self::Month => "month",
			Self::Year => "year",
			Self::All => "all",
		}
	}
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListingQuery {
	pub after: Option<String>,
	pub before: Option<String>,
	#[param(minimum = 1, maximum = 100)]
	pub limit: Option<u8>,
	#[param(minimum = 0)]
	pub count: Option<u32>,
	#[param(inline)]
	pub show: Option<ListingShow>,
	#[param(rename = "t", inline)]
	#[serde(rename = "t")]
	pub time: Option<ListingTime>,
	pub sr_detail: Option<bool>,
	#[param(rename = "g")]
	#[serde(rename = "g")]
	pub geo_filter: Option<String>,
}
