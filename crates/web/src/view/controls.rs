use super::{SelectChoice, SortControls};
use neddit_api::{search::Mode, service::ListingTime};

pub fn search_choices(active_kind: &str, active_sort: &str, active_limit: u8) -> (Vec<SelectChoice>, Vec<SelectChoice>, Vec<SelectChoice>) {
	let choices = |values: &[(&'static str, &'static str)], active: &str| {
		values
			.iter()
			.map(|&(value, label)| SelectChoice {
				value,
				label,
				checked: value == active,
			})
			.collect()
	};
	let mut sorts: Vec<SelectChoice> = choices(
		&[("relevance", "Relevance"), ("new", "New"), ("top", "Top"), ("hot", "Hot"), ("activity", "Activity")],
		active_sort,
	);
	let enabled = [Mode::Posts, Mode::Comments, Mode::Communities]
		.into_iter()
		.find(|mode| mode.as_str() == active_kind)
		.map(Mode::sorts)
		.unwrap_or_default();
	sorts.retain(|choice| enabled.contains(&choice.value));
	let selected = sorts.iter().find(|s| s.checked).or_else(|| sorts.first()).map(|s| s.value);
	for choice in &mut sorts {
		choice.checked = Some(choice.value) == selected;
	}
	(
		choices(&[("posts", "Posts"), ("comments", "Comments"), ("communities", "Communities")], active_kind),
		sorts,
		choices(&[("25", "25"), ("50", "50"), ("100", "100")], &active_limit.to_string()),
	)
}

pub fn feed_controls(action: &str, active_sort: &'static str, active_time: &str, include_controversial: bool) -> SortControls {
	let mut sorts = vec![("hot", "Hot"), ("new", "New"), ("rising", "Rising"), ("top", "Top")];
	if include_controversial {
		sorts.push(("controversial", "Controversial"));
	}
	SortControls {
		action: action.into(),
		label: "Post sorting",
		choice_label: "Sort posts",
		active_sort,
		query: String::new(),
		sorts: select_choices(sorts, active_sort),
		times: if active_sort == "top" { listing_time_choices(active_time) } else { Vec::new() },
	}
}

pub fn comment_sort_controls(active: &'static str, permalink: &str, query: &str) -> SortControls {
	SortControls {
		action: permalink.into(),
		label: "Comment sorting",
		choice_label: "Sort comments",
		active_sort: active,
		query: query.into(),
		sorts: select_choices(
			[("best", "Best"), ("top", "Top"), ("new", "New"), ("old", "Old"), ("controversial", "Controversial")],
			active,
		),
		times: Vec::new(),
	}
}

pub fn user_controls(action: &str, active_sort: &'static str, active_time: &str) -> SortControls {
	SortControls {
		action: action.into(),
		label: "User activity sorting",
		choice_label: "Sort user activity",
		active_sort,
		query: String::new(),
		sorts: select_choices([("new", "New"), ("hot", "Hot"), ("top", "Top"), ("controversial", "Controversial")], active_sort),
		times: if active_sort == "top" { listing_time_choices(active_time) } else { Vec::new() },
	}
}

fn select_choices(values: impl IntoIterator<Item = (&'static str, &'static str)>, active: &str) -> Vec<SelectChoice> {
	values
		.into_iter()
		.map(|(value, label)| SelectChoice {
			value,
			label,
			checked: value == active,
		})
		.collect()
}

pub(crate) fn listing_time_choices(active: &str) -> Vec<SelectChoice> {
	select_choices(
		[
			(ListingTime::Hour.as_str(), "Past hour"),
			(ListingTime::Day.as_str(), "Past 24 hours"),
			(ListingTime::Week.as_str(), "Past week"),
			(ListingTime::Month.as_str(), "Past month"),
			(ListingTime::Year.as_str(), "Past year"),
			(ListingTime::All.as_str(), "All time"),
		],
		active,
	)
}
