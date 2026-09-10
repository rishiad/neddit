use super::*;
use crate::service::ListingTime;
use crate::{
	client::Access,
	models::PublicThing,
	service::{InfoQuery, ListingQuery, RedditService, SearchQuery, SearchResultType, SearchSort, SubredditSearchQuery, SubredditSearchSort, UserHistoryQuery, UserHistorySort},
};
mod session;
pub use session::Sessions;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(default, deny_unknown_fields)]
pub struct Request {
	pub q: String,
	pub kind: Mode,
	pub sort: Option<String>,
	/// Reddit Top window: hour, day, week, month, year, or all. Defaults to all.
	#[param(inline)]
	pub t: Option<ListingTime>,
	pub limit: Option<u8>,
	/// Include adult results in every search mode. Defaults to false.
	pub include_nsfw: bool,
	pub cursor: Option<String>,
}
impl Request {
	pub fn sort(&self) -> &str {
		self.sort.as_deref().unwrap_or("relevance")
	}
	pub fn limit(&self) -> u8 {
		self.limit.unwrap_or(25)
	}
	pub fn top_time(&self) -> ListingTime {
		self.t.unwrap_or(ListingTime::All)
	}
	fn binding(&self) -> Self {
		let mut r = self.clone();
		r.cursor = None;
		r.limit = Some(self.limit());
		r.sort = Some(self.sort().into());
		if self.sort() == "top" {
			r.t = Some(self.top_time());
		}
		r
	}
}
#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Page {
	pub items: Vec<PublicThing>,
	/// One-based page number within the retained search session.
	pub number: u32,
	/// Cursor for the preceding immutable page.
	pub previous_cursor: Option<String>,
	pub cursor: Option<String>,
	pub ranking: String,
	/// Buffered results, possible further discovery, source exhaustion, or a terminal stop.
	pub continuation: String,
	pub coverage: Vec<String>,
	/// Cumulative confirmed matches at this page's creation, not an exact total.
	pub observed_matches: usize,
	pub frozen_at: String,
	pub text_profile: &'static str,
}
#[derive(Debug, PartialEq)]
enum Plan {
	Posts(String),
	Comments(Field, String),
	Communities(String),
	Names(Vec<String>),
}

fn quote(value: &str) -> String {
	format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

// None is the universal set, not an empty result. Polarity must precede erasure.
fn cover(expr: &Expr, negative: bool) -> Option<String> {
	match &expr.kind {
		Node::Not(e) => cover(e, !negative),
		Node::And(a, b) | Node::Or(a, b) => {
			let and = matches!(expr.kind, Node::And(..)) != negative;
			match (cover(a, negative), cover(b, negative)) {
				(Some(a), Some(b)) => Some(format!("({a} {} {b})", if and { "AND" } else { "OR" })),
				(Some(a), None) | (None, Some(a)) if and => Some(a),
				_ => None,
			}
		}
		Node::Predicate(p) if !negative => {
			let name = match p.field {
				Field::Text => "",
				Field::Subreddit => "subreddit:",
				Field::Author => "author:",
				Field::Title => "title:",
				Field::Domain => "site:",
				Field::Flair => "flair:",
				_ => return None,
			};
			let value = match &p.value {
				Value::Exact(v) => v,
				_ => &p.raw,
			};
			Some(format!("{name}{}", quote(value)))
		}
		_ => None,
	}
}
fn required(expr: &Expr, field: Field) -> Option<String> {
	match &expr.kind {
		Node::Predicate(p) if p.field == field => Some(match &p.value {
			Value::Exact(v) => v.clone(),
			_ => p.raw.clone(),
		}),
		Node::And(a, b) => required(a, field).or_else(|| required(b, field)),
		Node::Or(a, b) => required(a, field).filter(|v| required(b, field).as_ref() == Some(v)),
		_ => None,
	}
}
fn names(expr: &Expr) -> Option<Vec<String>> {
	match &expr.kind {
		Node::Predicate(p) if p.field == Field::Name => Some(vec![p.raw.to_ascii_lowercase()]),
		Node::And(a, b) => names(a).or_else(|| names(b)),
		Node::Or(a, b) => {
			let mut values = names(a)?;
			values.extend(names(b)?);
			Some(values)
		}
		_ => None,
	}
}
fn plan(request: &Request, expr: &Expr) -> Result<Plan> {
	let error = |code, message| Diagnostic::new(code, message, expr.span);
	if ![25, 50, 100].contains(&request.limit()) {
		return Err(error("invalid_value", "Page size must be 25, 50, or 100"));
	}
	let sort = request.sort();
	if request.t.is_some() && sort != "top" {
		return Err(error("invalid_value", "The t parameter applies only to Top; use QL date predicates for other sorts"));
	}
	if !request.kind.sorts().contains(&sort) {
		return Err(error("unsupported_sort", "This REST discovery plan does not support that sort"));
	}
	let plan = match request.kind {
		Mode::Posts => Plan::Posts(cover(expr, false).ok_or_else(|| error("scope_required", "Add a positive text, community, author, title, or domain scope"))?),
		Mode::Comments => {
			let scope = [Field::Author, Field::Subreddit].into_iter().find_map(|f| required(expr, f).map(|v| (f, v)));
			let (field, value) = scope.ok_or_else(|| {
				error(
					"unsupported_capability",
					"REST comments need a shared positive author: or subreddit: scope; global historical comment discovery is unavailable",
				)
			})?;
			Plan::Comments(field, value)
		}
		Mode::Communities => {
			if let Some(mut values) = names(expr) {
				values.sort();
				values.dedup();
				if values.len() > 100 {
					return Err(error("query_too_complex", "Exact community lookup limit: 100"));
				}
				if sort != "relevance" {
					return Err(error("unsupported_sort", "Exact community lookup supports only Neddit exact-name order"));
				}
				Plan::Names(values)
			} else {
				let value = [Field::Text, Field::Title, Field::Description].into_iter().find_map(|f| required(expr, f));
				Plan::Communities(value.ok_or_else(|| error("scope_required", "Add a shared positive community text predicate or exact name: scope"))?)
			}
		}
	};
	if matches!(&plan, Plan::Posts(q) | Plan::Communities(q) if q.len() > 512) {
		return Err(error("query_too_complex", "Compiled upstream query byte limit: 512"));
	}
	Ok(plan)
}

fn contradictory(expr: &Expr) -> bool {
	match &expr.kind {
		Node::And(a, b) => {
			[Field::Subreddit, Field::Author, Field::Name]
				.into_iter()
				.any(|f| matches!((required(a, f), required(b, f)), (Some(a), Some(b)) if !a.eq_ignore_ascii_case(&b)))
				|| contradictory(a)
				|| contradictory(b)
		}
		Node::Or(a, b) => contradictory(a) && contradictory(b),
		_ => false,
	}
}

impl RedditService {
	pub async fn search_ql(&self, request: &Request) -> Result<Page> {
		if !self.content.allows_nsfw() && request.include_nsfw {
			return Err(Diagnostic::new("content_blocked", "NSFW content is disabled by server policy", (0, 0)));
		}
		let mut request = request.clone();
		if !self.content.allows_nsfw() {
			request.include_nsfw = false;
		}
		let expr = parse(&request.q, request.kind)?;
		let plan = plan(&request, &expr)?;
		self.search_sessions.execute(self, &request, expr, plan).await
	}
}

type Upstream = std::result::Result<crate::models::Listing<PublicThing>, crate::service::ServiceError>;
trait Source: Sync {
	fn page(&self, plan: &Plan, request: &Request, listing: ListingQuery) -> impl std::future::Future<Output = Upstream> + Send;
	fn parents(&self, ids: &[String]) -> impl std::future::Future<Output = Upstream> + Send;
}
impl Source for RedditService {
	async fn page(&self, plan: &Plan, request: &Request, mut listing: ListingQuery) -> Upstream {
		let sort = request.sort();
		if matches!(plan, Plan::Posts(_)) && sort == "top" {
			listing.time = Some(request.top_time());
		}
		if matches!(plan, Plan::Comments(..)) {
			listing.sr_detail = Some(true);
		}
		match plan {
			Plan::Posts(q) => {
				self
					.search(
						&SearchQuery {
							listing,
							query: q.clone(),
							include_over_18: Some(request.include_nsfw),
							sort: Some(match sort {
								"new" => SearchSort::New,
								"top" => SearchSort::Top,
								"hot" => SearchSort::Hot,
								_ => SearchSort::Relevance,
							}),
							result_types: vec![SearchResultType::Post],
							..Default::default()
						},
						Access::Standard,
					)
					.await
			}
			Plan::Comments(Field::Author, author) => {
				self
					.user_comments(
						author,
						&UserHistoryQuery {
							listing,
							sort: Some(UserHistorySort::New),
							..Default::default()
						},
						Access::Standard,
					)
					.await
			}
			Plan::Comments(_, community) => self.recent_ql_comments(community, &listing).await,
			Plan::Communities(q) => self
				.search_subreddits(
					&SubredditSearchQuery {
						listing,
						query: q.clone(),
						search_query_id: None,
						show_users: Some(false),
						include_over_18: Some(request.include_nsfw),
						sort: Some(if sort == "activity" {
							SubredditSearchSort::Activity
						} else {
							SubredditSearchSort::Relevance
						}),
						typeahead_active: None,
					},
					Access::Standard,
				)
				.await
				.map(|v| crate::models::Listing {
					kind: v.kind,
					extra: Default::default(),
					data: crate::models::ListingData {
						children: v.data.children.into_iter().map(PublicThing::Subreddit).collect(),
						after: v.data.after,
						before: None,
						dist: None,
						modhash: None,
						geo_filter: None,
						extra: Default::default(),
					},
				}),
			Plan::Names(names) => {
				self
					.info(
						&InfoQuery {
							subreddit_names: names.clone(),
							..Default::default()
						},
						Access::Standard,
					)
					.await
			}
		}
	}
	async fn parents(&self, ids: &[String]) -> Upstream {
		self
			.info(
				&InfoQuery {
					ids: ids.to_vec(),
					..Default::default()
				},
				Access::Standard,
			)
			.await
	}
}

fn correct_kind(item: &PublicThing, mode: Mode) -> bool {
	matches!(
		(item, mode),
		(PublicThing::Post(_), Mode::Posts) | (PublicThing::Comment(_), Mode::Comments) | (PublicThing::Subreddit(_), Mode::Communities)
	)
}
fn identity(item: &PublicThing) -> &str {
	match item {
		PublicThing::Post(p) => &p.data.name,
		PublicThing::Comment(c) => &c.data.name,
		PublicThing::Subreddit(s) => &s.data.name,
		PublicThing::User(u) => &u.data.name,
	}
}
fn order_fields(item: &PublicThing) -> (f64, i64) {
	match item {
		PublicThing::Post(p) => (p.data.created_utc, p.data.score),
		PublicThing::Comment(c) => (c.data.created_utc, c.data.score),
		_ => (0.0, 0),
	}
}
fn needs_parent(expr: &Expr) -> bool {
	match &expr.kind {
		Node::Predicate(p) => matches!(p.field, Field::PostTitle | Field::PostDomain | Field::PostFlair),
		Node::Not(e) => needs_parent(e),
		Node::And(a, b) | Node::Or(a, b) => needs_parent(a) || needs_parent(b),
	}
}
fn record(item: &PublicThing) -> Record {
	if let PublicThing::Post(post) = item {
		return post_record(&post.data);
	}
	let mut r = Record::default();
	let mut put = |f, v| {
		r.fields.insert(f, v);
	};
	match item {
		PublicThing::Post(_) => unreachable!(),
		PublicThing::Comment(c) => {
			let c = &c.data;
			put(Field::Author, author(&c.author));
			put(Field::Subreddit, optional(&c.extra, "subreddit"));
			r.text = vec![body(&c.body)];
			r.created = seconds(c.created_utc);
		}
		PublicThing::Subreddit(s) => {
			let s = &s.data;
			put(Field::Name, Datum::Text(s.display_name.clone()));
			put(Field::Title, Datum::Text(s.title.clone()));
			put(Field::Description, Datum::Text(s.public_description.clone()));
			r.text = vec![Datum::Text(s.display_name.clone()), Datum::Text(s.title.clone()), Datum::Text(s.public_description.clone())];
		}
		_ => {}
	}
	r
}
