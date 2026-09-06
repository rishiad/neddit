#![forbid(unsafe_code)]

#[macro_export]
macro_rules! dbg_msg {
	($value:expr) => {
		#[cfg(debug_assertions)]
		eprintln!("{}:{}: {}", file!(), line!(), $value.to_string())
	};

	($($value:expr),+) => {
		#[cfg(debug_assertions)]
		$crate::dbg_msg!(format!($($value),+))
	};
}

pub mod api;
pub mod client;
pub mod media;
pub mod models;
pub mod parsing;
pub mod server;
pub mod service;
