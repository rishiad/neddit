use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt::Display, str::FromStr};

pub(super) fn encode(value: &impl Serialize) -> String {
	serde_urlencoded::to_string(value).expect("query models serialize to form data")
}

pub(crate) mod comma {
	use super::*;

	pub fn serialize<T: Display, S: Serializer>(values: &[T], serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(&values.iter().map(ToString::to_string).collect::<Vec<_>>().join(","))
	}

	pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
	where
		T: FromStr,
		T::Err: Display,
		D: Deserializer<'de>,
	{
		let value = String::deserialize(deserializer)?;
		value
			.split(',')
			.map(str::trim)
			.filter(|value| !value.is_empty())
			.map(|value| value.parse().map_err(D::Error::custom))
			.collect()
	}
}

pub(crate) mod option_on_off {
	use super::*;

	pub fn serialize<S: Serializer>(value: &Option<bool>, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(if value.unwrap_or_default() { "on" } else { "off" })
	}

	pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<bool>, D::Error> {
		match String::deserialize(deserializer)?.as_str() {
			"true" => Ok(Some(true)),
			"false" => Ok(Some(false)),
			value => Err(D::Error::custom(format_args!("invalid boolean `{value}`"))),
		}
	}
}
