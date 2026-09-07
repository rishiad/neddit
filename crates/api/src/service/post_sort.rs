#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PostSort {
	#[default]
	Hot,
	Best,
	New,
	Rising,
	Top,
	Controversial,
}

impl PostSort {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Hot => "hot",
			Self::Best => "best",
			Self::New => "new",
			Self::Rising => "rising",
			Self::Top => "top",
			Self::Controversial => "controversial",
		}
	}
}
