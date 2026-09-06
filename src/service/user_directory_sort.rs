#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserDirectorySort {
	New,
	Popular,
}

impl UserDirectorySort {
	pub(super) const fn as_str(self) -> &'static str {
		match self {
			Self::New => "new",
			Self::Popular => "popular",
		}
	}
}
