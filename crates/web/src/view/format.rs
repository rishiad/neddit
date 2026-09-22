use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn signed_compact(value: i64) -> String {
	if value < 0 {
		format!("−{}", compact(value.unsigned_abs()))
	} else {
		compact(value.unsigned_abs())
	}
}

pub(super) fn compact(value: u64) -> String {
	const UNITS: [(u64, &str); 7] = [
		(1, ""),
		(1_000, "k"),
		(1_000_000, "m"),
		(1_000_000_000, "b"),
		(1_000_000_000_000, "t"),
		(1_000_000_000_000_000, "q"),
		(1_000_000_000_000_000_000, "e"),
	];
	if value < 1_000 {
		return value.to_string();
	}

	let mut unit = UNITS.partition_point(|(divisor, _)| *divisor <= value) - 1;
	loop {
		let (divisor, suffix) = UNITS[unit];
		let value = u128::from(value);
		let divisor = u128::from(divisor);

		if value < divisor * 10 {
			let tenths = (value * 10 + divisor / 2) / divisor;
			let whole = tenths / 10;
			let decimal = tenths % 10;
			return if decimal == 0 {
				format!("{whole}{suffix}")
			} else {
				format!("{whole}.{decimal}{suffix}")
			};
		}

		let rounded = (value + divisor / 2) / divisor;
		if rounded >= 1_000 && unit + 1 < UNITS.len() {
			unit += 1;
			continue;
		}
		return format!("{rounded}{suffix}");
	}
}

pub(super) fn age(timestamp: f64) -> String {
	let elapsed = if timestamp.is_finite() {
		(unix_now() - timestamp).clamp(0.0, 1_000_000_000_000.0)
	} else {
		0.0
	};
	let seconds = std::time::Duration::from_secs_f64(elapsed).as_secs();
	for (name, size) in [("year", 31_536_000), ("month", 2_592_000), ("day", 86_400), ("hour", 3_600), ("minute", 60), ("second", 1)] {
		if seconds >= size || size == 1 {
			let count = seconds / size;
			return format!("{count} {name}{} ago", if count == 1 { "" } else { "s" });
		}
	}
	"now".into()
}

fn unix_now() -> f64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |duration| duration.as_secs_f64())
}
