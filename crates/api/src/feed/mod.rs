//! Custom post feeds with immutable definitions and bounded snapshots.
mod ranking;
mod shortlinks;
pub use shortlinks::{status, Continuation, Definition, SavedFeed};

use crate::{
	client::Access,
	models::{Listing, Post, Thing},
	search::{parse, post_record, Diagnostic, Expr, Field, Mode, Node, Truth, Value, TEXT_PROFILE},
	service::{ListingQuery, ListingTime, PostSort, RedditService, ServiceError, UserHistoryQuery, UserHistorySort},
};
use chrono::{DateTime, Utc};
use ranking::Program;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Duration;

const MAX_CANDIDATES: usize = 400;
const MAX_MATCH_BYTES: usize = 2 * 1024 * 1024;
const MAX_SOURCES: usize = 8;
const MAX_SUBREDDITS: usize = 100;
const DEADLINE: Duration = Duration::from_secs(20);

const HOT_EXPRESSION: &str = "sign(score) * log10(max(abs(score), 1)) - age_hours / 12.5";
const NEW_EXPRESSION: &str = "-age_seconds";
const TOP_EXPRESSION: &str = "score";
const DISCUSSION_EXPRESSION: &str = "log1p(comments) - age_hours / 24";
const DIVISIVE_EXPRESSION: &str = "max(0, 1 - 2 * abs(upvote_ratio - 0.5)) * log1p(comments) - age_hours / 168";

pub fn expand_rank_preset(value: &str) -> &str {
	match value {
		"new" => NEW_EXPRESSION,
		"top" => TOP_EXPRESSION,
		"hot" => HOT_EXPRESSION,
		"discussion" => DISCUSSION_EXPRESSION,
		"divisive" => DIVISIVE_EXPRESSION,
		formula => formula,
	}
}

pub const RANKING_INPUTS: &[&str] = &[
	"score",
	"upvote_ratio",
	"comments",
	"created_utc",
	"frozen_at",
	"age_seconds",
	"age_hours",
	"source_rank",
	"awards",
	"crossposts",
	"score_hidden",
	"is_self",
	"over_18",
	"spoiler",
	"stickied",
	"pinned",
	"locked",
	"archived",
];

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq, utoipa::IntoParams, utoipa::ToSchema)]
#[into_params(parameter_in = Query)]
#[serde(default, deny_unknown_fields)]
pub struct Request {
	pub q: String,
	/// Native candidate pool: new or top. Defaults to new.
	pub pool: Option<String>,
	/// Reddit Top window. Defaults to all.
	#[param(inline)]
	pub t: Option<ListingTime>,
	/// A named preset or numeric expression. Defaults to hot. Results use descending values.
	pub rank: Option<String>,
	#[param(minimum = 1, maximum = 100)]
	pub limit: Option<u8>,
	#[param(minimum = 1)]
	pub page: Option<u32>,
	/// Opaque snapshot identifier returned by the first page. Expires after 15 minutes or eviction.
	pub cursor: Option<String>,
	pub include_nsfw: bool,
}

impl Request {
	pub fn pool(&self) -> &str {
		self.pool.as_deref().unwrap_or("new")
	}

	pub fn rank(&self) -> &str {
		self.rank.as_deref().map(str::trim).filter(|value| !value.is_empty()).unwrap_or("hot")
	}

	pub fn limit(&self) -> u8 {
		self.limit.unwrap_or(25)
	}

	pub fn page(&self) -> u32 {
		self.page.unwrap_or(1)
	}

	pub fn time(&self) -> ListingTime {
		self.t.unwrap_or(ListingTime::All)
	}
}

#[derive(Clone, Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct FeedPage {
	pub items: Vec<Item>,
	pub number: u32,
	pub previous_page: Option<u32>,
	pub next_page: Option<u32>,
	pub ranking: RankingDescription,
	pub sources: Vec<SourceDescription>,
	pub candidates_inspected: usize,
	pub observed_matches: usize,
	pub frozen_at: String,
	pub cursor: Option<String>,
	pub coverage: Vec<String>,
	pub text_profile: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct Item {
	pub post: Thing<Post>,
	pub signals: RankingSignals,
}

#[derive(Clone, Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RankingSignals {
	pub score: i64,
	pub upvote_ratio: f64,
	pub comments: u64,
	pub created_utc: f64,
	pub frozen_at: f64,
	pub age_seconds: f64,
	pub age_hours: f64,
	pub source: String,
	pub source_pool: String,
	pub source_rank: u32,
	pub awards: u64,
	pub crossposts: u64,
	pub score_hidden: bool,
	pub is_self: bool,
	pub over_18: bool,
	pub spoiler: bool,
	pub stickied: bool,
	pub pinned: bool,
	pub locked: bool,
	pub archived: bool,
	pub rank_value: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RankingDescription {
	pub name: String,
	pub expression: Option<String>,
	pub direction: String,
	pub inputs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct SourceDescription {
	pub kind: String,
	pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidatePool {
	New,
	Top,
}

impl CandidatePool {
	fn parse(request: &Request, span: (usize, usize)) -> Result<Self, Diagnostic> {
		let pool = match request.pool() {
			"new" => Self::New,
			"top" => Self::Top,
			_ => return Err(Diagnostic::new("unsupported_pool", "Candidate pool must be new or top", span)),
		};
		if request.t.is_some() && pool != Self::Top {
			return Err(Diagnostic::new("invalid_value", "The t parameter applies only to the Top candidate pool", span));
		}
		Ok(pool)
	}

	const fn as_str(self) -> &'static str {
		match self {
			Self::New => "new",
			Self::Top => "top",
		}
	}

	const fn post_sort(self) -> PostSort {
		match self {
			Self::New => PostSort::New,
			Self::Top => PostSort::Top,
		}
	}

	const fn user_sort(self) -> UserHistorySort {
		match self {
			Self::New => UserHistorySort::New,
			Self::Top => UserHistorySort::Top,
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Source {
	Subreddits(Vec<String>),
	Author(String),
}

impl Source {
	fn name(&self) -> String {
		match self {
			Self::Subreddits(names) => format!("subreddit:{}", names.join("+")),
			Self::Author(name) => format!("author:{name}"),
		}
	}

	fn description(&self) -> SourceDescription {
		match self {
			Self::Subreddits(names) => SourceDescription {
				kind: "subreddits".into(),
				value: names.join("+"),
			},
			Self::Author(name) => SourceDescription {
				kind: "author".into(),
				value: name.clone(),
			},
		}
	}
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SourcePlan {
	subreddits: BTreeSet<String>,
	authors: BTreeSet<String>,
}

impl SourcePlan {
	fn predicate(field: Field, value: &str) -> Option<Self> {
		let mut plan = Self::default();
		match field {
			Field::Subreddit => plan.subreddits.insert(value.into()),
			Field::Author => plan.authors.insert(value.into()),
			_ => return None,
		};
		Some(plan)
	}

	fn union(mut self, other: Self) -> Self {
		self.subreddits.extend(other.subreddits);
		self.authors.extend(other.authors);
		self
	}

	fn cost(&self) -> usize {
		self.authors.len() + usize::from(!self.subreddits.is_empty())
	}

	fn sources(&self) -> Vec<Source> {
		let mut sources = Vec::with_capacity(self.cost());
		if !self.subreddits.is_empty() {
			sources.push(Source::Subreddits(self.subreddits.iter().cloned().collect()));
		}
		sources.extend(self.authors.iter().cloned().map(Source::Author));
		sources
	}
}

fn source_cover(expression: &Expr, negative: bool) -> Option<SourcePlan> {
	match &expression.kind {
		Node::Not(value) => source_cover(value, !negative),
		Node::And(left, right) | Node::Or(left, right) => {
			let intersection = matches!(expression.kind, Node::And(..)) != negative;
			let left = source_cover(left, negative);
			let right = source_cover(right, negative);
			if intersection {
				choose_cover(left, right)
			} else {
				Some(left?.union(right?))
			}
		}
		Node::Predicate(predicate) if !negative => {
			let Value::Exact(value) = &predicate.value else {
				return None;
			};
			SourcePlan::predicate(predicate.field, value)
		}
		Node::Predicate(_) => None,
	}
}

fn choose_cover(left: Option<SourcePlan>, right: Option<SourcePlan>) -> Option<SourcePlan> {
	match (left, right) {
		(Some(left), Some(right)) => {
			let left_key = (left.cost(), usize::from(!left.subreddits.is_empty()), left.subreddits.len() + left.authors.len());
			let right_key = (right.cost(), usize::from(!right.subreddits.is_empty()), right.subreddits.len() + right.authors.len());
			Some(if left_key <= right_key { left } else { right })
		}
		(Some(plan), None) | (None, Some(plan)) => Some(plan),
		(None, None) => None,
	}
}

fn plan(expression: &Expr) -> Result<Vec<Source>, Diagnostic> {
	let plan = source_cover(expression, false)
		.ok_or_else(|| Diagnostic::new("scope_required", "Add a positive author: or subreddit: source that covers every OR branch", expression.span))?;
	if plan.subreddits.len() > MAX_SUBREDDITS {
		return Err(Diagnostic::new("query_too_complex", "Combined subreddit source limit: 100", expression.span));
	}
	if plan.cost() > MAX_SOURCES {
		return Err(Diagnostic::new("query_too_complex", "Independent feed source limit: 8", expression.span));
	}
	Ok(plan.sources())
}

#[derive(Debug)]
struct Ranking {
	name: String,
	expression: String,
	program: Program,
}

impl Ranking {
	fn resolve(request: &Request) -> Result<Self, Diagnostic> {
		let value = expand_rank_preset(request.rank());
		match value {
			NEW_EXPRESSION => Self::preset("neddit.new-v1", NEW_EXPRESSION),
			TOP_EXPRESSION => Self::preset("neddit.top-v1", TOP_EXPRESSION),
			HOT_EXPRESSION => Self::preset("neddit.legacy-hot-v1", HOT_EXPRESSION),
			DISCUSSION_EXPRESSION => Self::preset("neddit.discussion-v1", DISCUSSION_EXPRESSION),
			DIVISIVE_EXPRESSION => Self::preset("neddit.divisive-v1", DIVISIVE_EXPRESSION),
			formula => Ok(Self {
				name: "neddit.custom-v1".into(),
				expression: formula.into(),
				program: Program::parse(formula)?,
			}),
		}
	}

	fn preset(name: &str, expression: &str) -> Result<Self, Diagnostic> {
		Ok(Self {
			name: name.into(),
			expression: expression.into(),
			program: Program::parse(expression)?,
		})
	}

	fn description(&self) -> RankingDescription {
		RankingDescription {
			name: self.name.clone(),
			expression: Some(self.expression.clone()),
			direction: "descending".into(),
			inputs: RANKING_INPUTS.iter().map(|s| (*s).into()).collect(),
		}
	}
}

type Upstream = Result<Listing<Thing<Post>>, ServiceError>;

trait CandidateSource: Sync {
	fn page(&self, source: &Source, pool: CandidatePool, query: ListingQuery) -> impl std::future::Future<Output = Upstream> + Send;
}

impl CandidateSource for RedditService {
	async fn page(&self, source: &Source, pool: CandidatePool, query: ListingQuery) -> Upstream {
		match source {
			Source::Subreddits(names) => self.feed_posts(&names.join("+"), pool.post_sort(), &query).await,
			Source::Author(author) => {
				self
					.user_submitted(
						author,
						&UserHistoryQuery {
							listing: query,
							sort: Some(pool.user_sort()),
							..Default::default()
						},
						Access::Standard,
					)
					.await
			}
		}
	}
}

struct Candidate {
	post: Thing<Post>,
	signals: RankingSignals,
}

struct Collection {
	candidates: Vec<Candidate>,
	inspected: usize,
	unknown: usize,
	bounded: bool,
	all_sources_exhausted: bool,
}

impl RedditService {
	pub async fn custom_feed(&self, request: &Request) -> Result<FeedPage, Diagnostic> {
		tokio::time::timeout(DEADLINE, self.cached_feed(request))
			.await
			.map_err(|_| Diagnostic::new("execution_timeout", "Feed deadline exceeded: 20 seconds", (0, 0)))?
	}

	async fn cached_feed(&self, request: &Request) -> Result<FeedPage, Diagnostic> {
		if !self.content.allows_nsfw() && request.include_nsfw {
			return Err(Diagnostic::new("content_blocked", "NSFW content is disabled by server policy", (0, 0)));
		}
		let definition = request.definition()?;
		if request.page() == 0 {
			return Err(Diagnostic::new("invalid_value", "Feed page must be positive", (0, 0)));
		}
		let start = request
			.page()
			.saturating_sub(1)
			.checked_mul(u32::from(request.limit()))
			.and_then(|value| usize::try_from(value).ok())
			.filter(|value| *value < MAX_CANDIDATES)
			.ok_or_else(|| Diagnostic::new("invalid_value", "Feed page starts beyond the 400-candidate window", (0, 0)))?;
		let binding = crate::storage::fingerprint(format!("feed-v1:{TEXT_PROFILE}:{}:{}", self.allows_nsfw(), definition.canonical()).as_bytes());
		let snapshot_key = |cursor: &str| format!("snapshot:{binding}:{cursor}");
		if let Some(cursor) = &request.cursor {
			if cursor.len() != 24 || !cursor.bytes().all(|b| b.is_ascii_alphanumeric()) {
				return Err(expired_cursor());
			}
			let bytes = match &self.cache {
				Some(cache) => cache.get(&snapshot_key(cursor)).await,
				None => None,
			}
			.ok_or_else(expired_cursor)?;
			let page = serde_json::from_slice(&bytes).map_err(|_| expired_cursor())?;
			return Ok(slice_page(page, request, start));
		}
		if request.page() != 1 {
			return Err(expired_cursor());
		}
		let _flight = self.feed_flights.lock(&binding).await;
		let head_key = format!("feed-head:{binding}");
		if let Some(cache) = &self.cache {
			if let Some(head) = cache.get(&head_key).await {
				if let Ok(cursor) = String::from_utf8(head) {
					if let Some(bytes) = cache.get(&snapshot_key(&cursor)).await {
						if let Ok(page) = serde_json::from_slice(&bytes) {
							return Ok(slice_page(page, request, start));
						}
					}
				}
			}
		}
		let _permit = self
			.feed_gate
			.acquire()
			.await
			.map_err(|_| Diagnostic::new("execution_busy", "Feed executor unavailable", (0, 0)))?;
		let mut page = self.build_feed(request).await?;
		if let Some(cache) = &self.cache {
			let cursor = crate::storage::id(24);
			page.cursor = Some(cursor.clone());
			let payload = serde_json::to_vec(&page).map_err(|_| Diagnostic::new("execution_busy", "Cannot serialize feed snapshot", (0, 0)))?;
			match cache.put(snapshot_key(&cursor), payload, 900).await {
				Ok(()) => {
					let _ = cache.put(head_key, cursor.into_bytes(), 30).await;
				}
				Err(error) => {
					tracing::warn!(event = "feed.snapshot_failed", error = %error, "feed snapshot storage failed");
					page.cursor = None;
				}
			}
		}
		if page.cursor.is_none() {
			page.coverage.push("Snapshot storage unavailable; pagination is disabled. Retry the first page.".into());
		}
		Ok(slice_page(page, request, start))
	}

	async fn build_feed(&self, request: &Request) -> Result<FeedPage, Diagnostic> {
		let expression = parse(&request.q, Mode::Posts)?;
		let pool = CandidatePool::parse(request, expression.span)?;
		let sources = plan(&expression)?;
		let ranking = Ranking::resolve(request)?;
		let frozen = std::time::SystemTime::now().into();
		let execution = collect(self, request, &expression, &sources, pool, frozen);
		let mut collection = execution.await?;

		let frozen_ns = timestamp_ns(frozen);
		for candidate in &mut collection.candidates {
			candidate.signals.rank_value = Some(ranking.program.evaluate(&candidate.signals, &post_record(&candidate.post.data), frozen_ns)?);
		}
		collection.candidates.sort_by(|left, right| {
			right
				.signals
				.rank_value
				.unwrap_or_default()
				.total_cmp(&left.signals.rank_value.unwrap_or_default())
				.then_with(|| right.post.data.created_utc.total_cmp(&left.post.data.created_utc))
				.then_with(|| left.post.data.name.cmp(&right.post.data.name))
		});

		let total = collection.candidates.len();
		let items = collection
			.candidates
			.into_iter()
			.map(|candidate| Item {
				post: candidate.post,
				signals: candidate.signals,
			})
			.collect();
		let mut coverage = vec![
			"Feed discovery inspects at most 400 posts across bounded native Reddit listings; observed matches are not an exact total.".into(),
			"Pages use one immutable snapshot. Its cursor expires after 15 minutes or cache eviction.".into(),
		];
		if collection.bounded || !collection.all_sources_exhausted {
			coverage.push("At least one source had more posts outside its allocated candidate window.".into());
		}
		if collection.unknown > 0 {
			coverage.push(format!("{} candidates did not publish because required QL metadata was unavailable.", collection.unknown));
		}
		if !request.include_nsfw {
			coverage.push("NSFW posts are excluded.".into());
		}
		coverage.push("Scores, ratios, awards, and crosspost counts are mutable Reddit observations; visible vote scores may be fuzzed.".into());
		Ok(FeedPage {
			items,
			number: 1,
			previous_page: None,
			next_page: None,
			ranking: ranking.description(),
			sources: sources.iter().map(Source::description).collect(),
			candidates_inspected: collection.inspected,
			observed_matches: total,
			frozen_at: frozen.to_rfc3339(),
			cursor: None,
			coverage,
			text_profile: TEXT_PROFILE.into(),
		})
	}
}

fn expired_cursor() -> Diagnostic {
	Diagnostic::new(
		"invalid_cursor",
		"Feed cursor is missing, expired, evicted, or belongs to another definition; restart at page one",
		(0, 0),
	)
}

fn slice_page(mut page: FeedPage, request: &Request, start: usize) -> FeedPage {
	let total = page.items.len();
	page.items = page.items.into_iter().skip(start).take(usize::from(request.limit())).collect();
	page.number = request.page();
	page.previous_page = (request.page() > 1 && page.cursor.is_some()).then(|| request.page() - 1);
	page.next_page = (start + usize::from(request.limit()) < total && page.cursor.is_some()).then(|| request.page() + 1);
	page
}

async fn collect(
	service: &impl CandidateSource,
	request: &Request,
	expression: &Expr,
	sources: &[Source],
	pool: CandidatePool,
	frozen: DateTime<Utc>,
) -> Result<Collection, Diagnostic> {
	let quota = MAX_CANDIDATES.div_ceil(sources.len());
	let frozen_seconds = frozen.timestamp() as f64 + f64::from(frozen.timestamp_subsec_nanos()) / 1_000_000_000.0;
	let frozen_ns = timestamp_ns(frozen);
	let mut candidates: Vec<Candidate> = Vec::new();
	let mut positions = HashMap::<String, usize>::new();
	let mut inspected = 0;
	let mut unknown = 0;
	let mut matched_bytes: usize = 0;
	let mut bounded = false;
	let mut all_sources_exhausted = true;

	'next_source: for source in sources {
		let mut after = None;
		let mut seen_cursors = HashSet::new();
		let mut source_inspected = 0;
		let mut source_rank = 0_u32;
		loop {
			let remaining = quota.min(MAX_CANDIDATES - inspected).saturating_sub(source_inspected);
			if remaining == 0 {
				bounded = true;
				all_sources_exhausted = false;
				continue 'next_source;
			}
			let limit = u8::try_from(remaining.min(100)).expect("source page limit is bounded");
			let listing = service
				.page(
					source,
					pool,
					ListingQuery {
						after: after.clone(),
						limit: Some(limit),
						time: (pool == CandidatePool::Top).then(|| request.time()),
						..Default::default()
					},
				)
				.await
				.map_err(|_| Diagnostic::new("source_failed", format!("Reddit feed source failed: {}", source.name()), expression.span))?;
			if listing.data.children.len() > 100 {
				return Err(Diagnostic::new("source_failed", "Reddit source exceeded the 100-post page limit", expression.span));
			}
			let count = listing.data.children.len();
			for post in listing.data.children.into_iter().take(remaining) {
				inspected += 1;
				source_inspected += 1;
				source_rank += 1;
				if !request.include_nsfw && post.data.over_18 {
					continue;
				}
				match expression.evaluate(&post_record(&post.data), frozen_ns) {
					Truth::True => {}
					Truth::False => continue,
					Truth::Unknown => {
						unknown += 1;
						continue;
					}
				}
				let name = post.data.name.clone();
				if let Some(&index) = positions.get(&name) {
					if source_rank < candidates[index].signals.source_rank {
						candidates[index].signals.source = source.name();
						candidates[index].signals.source_rank = source_rank;
					}
					continue;
				}
				let bytes = serde_json::to_vec(&post).map_or(MAX_MATCH_BYTES + 1, |value| value.len());
				if matched_bytes.saturating_add(bytes) > MAX_MATCH_BYTES {
					bounded = true;
					all_sources_exhausted = false;
					break 'next_source;
				}
				matched_bytes += bytes;
				positions.insert(name, candidates.len());
				candidates.push(Candidate {
					signals: RankingSignals::new(&post.data, source.name(), pool, source_rank, frozen_seconds),
					post,
				});
			}
			after = listing.data.after;
			if after.is_none() || count == 0 {
				break;
			}
			let cursor = after.clone().unwrap();
			if cursor.len() > 128 || !seen_cursors.insert(cursor) {
				return Err(Diagnostic::new("source_failed", "Reddit source returned an invalid or repeated cursor", expression.span));
			}
		}
	}
	Ok(Collection {
		candidates,
		inspected,
		unknown,
		bounded,
		all_sources_exhausted,
	})
}

impl RankingSignals {
	fn new(post: &Post, source: String, pool: CandidatePool, source_rank: u32, frozen: f64) -> Self {
		let age_seconds = (frozen - post.created_utc).max(0.0);
		Self {
			score: post.score,
			upvote_ratio: post.upvote_ratio,
			comments: post.num_comments,
			created_utc: post.created_utc,
			frozen_at: frozen,
			age_seconds,
			age_hours: age_seconds / 3600.0,
			source,
			source_pool: pool.as_str().into(),
			source_rank,
			awards: extra_count(post, "total_awards_received"),
			crossposts: extra_count(post, "num_crossposts"),
			score_hidden: post.hide_score,
			is_self: post.is_self,
			over_18: post.over_18,
			spoiler: post.spoiler,
			stickied: post.stickied,
			pinned: post.pinned,
			locked: post.locked,
			archived: post.archived,
			rank_value: None,
		}
	}
}

fn extra_count(post: &Post, key: &str) -> u64 {
	post.extra.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0)
}

fn timestamp_ns(value: DateTime<Utc>) -> i128 {
	i128::from(value.timestamp()) * 1_000_000_000 + i128::from(value.timestamp_subsec_nanos())
}

