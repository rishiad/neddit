#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubredditSort {
	Popular,
	New,
	Default,
}

impl SubredditSort {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Popular => "popular",
			Self::New => "new",
			Self::Default => "default",
		}
	}
}
