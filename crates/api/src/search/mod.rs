//! QL syntax, validation, and deterministic local predicates.
mod engine;
pub use engine::{Page, Request, Sessions};

use crate::models::Post;
use caseless::Caseless;
use chrono::{DateTime, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::LazyLock};
use unicode_normalization::UnicodeNormalization;

pub const MAX_BYTES: usize = 4096;
pub const MAX_NODES: usize = 256;
pub const MAX_DEPTH: usize = 32;
pub const TEXT_PROFILE: &str = "nfc-caseless-0.2.2-nfc-0.1.25-ql1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
	#[default]
	Posts,
	Comments,
	Communities,
}
impl Mode {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Posts => "posts",
			Self::Comments => "comments",
			Self::Communities => "communities",
		}
	}
	pub const fn sorts(self) -> &'static [&'static str] {
		match self {
			Self::Posts => &["relevance", "new", "top", "hot"],
			Self::Comments => &["new", "top"],
			Self::Communities => &["relevance", "activity"],
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{code}: {message} (bytes {start}..{end})")]
pub struct Diagnostic {
	pub code: &'static str,
	pub message: String,
	pub start: usize,
	pub end: usize,
}
impl Diagnostic {
	pub(crate) fn new(code: &'static str, message: impl Into<String>, span: (usize, usize)) -> Self {
		Self {
			code,
			message: message.into(),
			start: span.0,
			end: span.1,
		}
	}

	pub fn status(&self) -> axum::http::StatusCode {
		use axum::http::StatusCode;
		match self.code {
			"not_found" => StatusCode::NOT_FOUND,
			"rate_limited" => StatusCode::TOO_MANY_REQUESTS,
			"storage_full" | "storage_unavailable" | "execution_busy" => StatusCode::SERVICE_UNAVAILABLE,
			"invalid_cursor" => StatusCode::GONE,
			"source_failed" => StatusCode::BAD_GATEWAY,
			"execution_timeout" => StatusCode::GATEWAY_TIMEOUT,
			_ => StatusCode::BAD_REQUEST,
		}
	}
}
type Result<T> = std::result::Result<T, Diagnostic>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
	pub kind: Node,
	pub span: (usize, usize),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
	Predicate(Predicate),
	Not(Box<Expr>),
	And(Box<Expr>, Box<Expr>),
	Or(Box<Expr>, Box<Expr>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
	Text,
	Subreddit,
	Author,
	Title,
	Domain,
	Flair,
	PostTitle,
	PostDomain,
	PostFlair,
	Name,
	Description,
	Past,
	After,
	Before,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Predicate {
	pub field: Field,
	pub raw: String,
	pub value: Value,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
	Text(Vec<String>),
	Exact(String),
	Duration(i128),
	Instant(i128),
}

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
	Word(String),
	Phrase(String),
	And,
	Or,
	Not,
	Open,
	Close,
}
#[derive(Clone, Debug)]
struct Token {
	kind: TokenKind,
	span: (usize, usize),
}

fn lex(source: &str) -> Result<Vec<Token>> {
	let mut out = Vec::new();
	let mut i = 0;
	while i < source.len() {
		let c = source[i..].chars().next().unwrap();
		if c.is_whitespace() {
			i += c.len_utf8();
			continue;
		}
		let start = i;
		let kind = if source[i..].starts_with("&&") {
			i += 2;
			TokenKind::And
		} else if source[i..].starts_with("||") {
			i += 2;
			TokenKind::Or
		} else {
			match c {
				'(' => {
					i += 1;
					TokenKind::Open
				}
				')' => {
					i += 1;
					TokenKind::Close
				}
				'!' => {
					i += 1;
					TokenKind::Not
				}
				'"' => {
					i += 1;
					let mut value = String::new();
					let mut closed = false;
					while i < source.len() {
						let c = source[i..].chars().next().unwrap();
						i += c.len_utf8();
						match c {
							'"' => {
								closed = true;
								break;
							}
							'\\' => {
								let escape = i - 1;
								let next = source[i..]
									.chars()
									.next()
									.ok_or_else(|| Diagnostic::new("invalid_escape", "Expected a quote or backslash escape", (escape, i)))?;
								i += next.len_utf8();
								if next != '"' && next != '\\' {
									return Err(Diagnostic::new("invalid_escape", "Only quotes and backslashes may be escaped", (escape, i)));
								}
								value.push(next);
							}
							_ => value.push(c),
						}
					}
					if !closed {
						return Err(Diagnostic::new("unterminated_quote", "Close the quoted value", (start, i)));
					}
					TokenKind::Phrase(value)
				}
				_ => {
					while i < source.len() {
						let c = source[i..].chars().next().unwrap();
						if c.is_whitespace() || matches!(c, '(' | ')' | '"') || source[i..].starts_with("&&") || source[i..].starts_with("||") {
							break;
						}
						if c == '\\' {
							return Err(Diagnostic::new("invalid_escape", "Backslashes require a quoted value", (i, i + 1)));
						}
						i += c.len_utf8();
					}
					let word = &source[start..i];
					match word {
						"AND" => TokenKind::And,
						"OR" => TokenKind::Or,
						"NOT" => TokenKind::Not,
						"&" | "|" | "^" | "=" | "==" | "!=" | ">" | "<" | ">=" | "<=" => return Err(Diagnostic::new("unexpected_token", "Unsupported operator", (start, i))),
						_ => TokenKind::Word(word.into()),
					}
				}
			}
		};
		out.push(Token { kind, span: (start, i) });
		if out.len() > MAX_NODES * 3 {
			return Err(Diagnostic::new("query_too_complex", "Token limit: 768", (start, i)));
		}
	}
	Ok(out)
}

fn qualification(word: &str) -> Option<(&str, &str)> {
	let (name, value) = word.split_once(':')?;
	(name.starts_with(|c: char| c.is_ascii_alphabetic()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')).then_some((name, value))
}

pub fn parse(source: &str, mode: Mode) -> Result<Expr> {
	if source.len() > MAX_BYTES {
		return Err(Diagnostic::new("query_too_complex", "Source byte limit: 4096", (0, source.len())));
	}
	let tokens = lex(source)?;
	if tokens.is_empty() {
		return Err(Diagnostic::new("empty_query", "Enter a query", (0, source.len())));
	}
	let mut parser = Parser {
		tokens,
		index: 0,
		mode,
		nodes: 0,
		end: source.len(),
	};
	let expr = parser.expression(0, None, 0)?;
	if let Some(token) = parser.tokens.get(parser.index) {
		return Err(Diagnostic::new(
			if token.kind == TokenKind::Close { "unbalanced_group" } else { "unexpected_token" },
			"Expected an operator or whitespace",
			token.span,
		));
	}
	Ok(expr)
}

struct Parser {
	tokens: Vec<Token>,
	index: usize,
	mode: Mode,
	nodes: usize,
	end: usize,
}
impl Parser {
	fn node(&mut self, kind: Node, span: (usize, usize)) -> Result<Expr> {
		self.nodes += 1;
		if self.nodes > MAX_NODES {
			return Err(Diagnostic::new("query_too_complex", "Expression node limit: 256", span));
		}
		Ok(Expr { kind, span })
	}
	fn expression(&mut self, min: u8, field: Option<Field>, depth: usize) -> Result<Expr> {
		let mut left = self.primary(field, depth)?;
		while let Some(token) = self.tokens.get(self.index) {
			let (precedence, explicit) = match token.kind {
				TokenKind::Or => (1, true),
				TokenKind::And => (2, true),
				TokenKind::Close => break,
				_ if token.span.0 > self.tokens[self.index - 1].span.1 => (2, false),
				_ => break,
			};
			if precedence < min {
				break;
			}
			if explicit {
				self.index += 1;
			}
			let right = self.expression(precedence + 1, field, depth + 1)?;
			let span = (left.span.0, right.span.1);
			left = self.node(
				if precedence == 1 {
					Node::Or(Box::new(left), Box::new(right))
				} else {
					Node::And(Box::new(left), Box::new(right))
				},
				span,
			)?;
		}
		Ok(left)
	}
	fn primary(&mut self, field: Option<Field>, depth: usize) -> Result<Expr> {
		let token = self
			.tokens
			.get(self.index)
			.cloned()
			.ok_or_else(|| Diagnostic::new("missing_operand", "Expected an operand", (self.end, self.end)))?;
		if depth > MAX_DEPTH {
			return Err(Diagnostic::new("query_too_complex", "Nesting limit: 32", token.span));
		}
		self.index += 1;
		match token.kind {
			TokenKind::Not => {
				let value = self.primary(field, depth + 1)?;
				let span = (token.span.0, value.span.1);
				self.node(Node::Not(Box::new(value)), span)
			}
			TokenKind::Open => {
				let mut expr = self.expression(0, field, depth + 1)?;
				let close = self
					.tokens
					.get(self.index)
					.filter(|t| t.kind == TokenKind::Close)
					.ok_or_else(|| Diagnostic::new("unbalanced_group", "Close the group", token.span))?;
				expr.span = (token.span.0, close.span.1);
				self.index += 1;
				Ok(expr)
			}
			TokenKind::Word(word) => {
				if let Some((name, value)) = qualification(&word) {
					if field.is_some() {
						return Err(Diagnostic::new("unexpected_token", "Field groups cannot contain qualified predicates", token.span));
					}
					let target = field_name(name, self.mode, (token.span.0, token.span.0 + name.len()))?;
					if !value.is_empty() {
						if value.starts_with('!') || matches!(value, "AND" | "OR" | "NOT" | "&" | "|" | "^" | "=" | "==" | ">=" | "<=" | ">" | "<" | "!=") {
							return Err(Diagnostic::new("unexpected_token", "Quote a literal operator value", token.span));
						}
						return self.scalar(target, value.into(), token.span);
					}
					let next = self
						.tokens
						.get(self.index)
						.ok_or_else(|| Diagnostic::new("missing_operand", "Expected a field value", token.span))?;
					if next.kind == TokenKind::Open {
						return self.primary(Some(target), depth + 1);
					}
					let next = next.clone();
					self.index += 1;
					return match next.kind {
						TokenKind::Word(v) | TokenKind::Phrase(v) => self.scalar(target, v, (token.span.0, next.span.1)),
						_ => Err(Diagnostic::new("missing_operand", "Expected a scalar or value group", next.span)),
					};
				}
				self.scalar(field.unwrap_or(Field::Text), word, token.span)
			}
			TokenKind::Phrase(value) => self.scalar(field.unwrap_or(Field::Text), value, token.span),
			_ => Err(Diagnostic::new("unexpected_token", "Expected a value or group", token.span)),
		}
	}
	fn scalar(&mut self, field: Field, raw: String, span: (usize, usize)) -> Result<Expr> {
		let invalid = || Diagnostic::new("invalid_value", format!("Invalid value for {field:?}"), span);
		let value = match field {
			Field::Subreddit | Field::Name | Field::Author => {
				let max = if field == Field::Author { 20 } else { 21 };
				if raw.is_empty() || raw.len() > max || !raw.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || (field == Field::Author && c == b'-')) {
					return Err(invalid());
				}
				Value::Exact(raw.to_ascii_lowercase())
			}
			Field::Domain | Field::PostDomain => Value::Exact(domain(&raw).ok_or_else(invalid)?),
			Field::Past => {
				let multiplier: i64 = match raw.as_bytes().last() {
					Some(b'm') => 60,
					Some(b'h') => 3600,
					Some(b'd') => 86400,
					Some(b'w') => 604800,
					_ => return Err(invalid()),
				};
				let number = &raw[..raw.len() - 1];
				if number.is_empty() || !number.bytes().all(|c| c.is_ascii_digit()) {
					return Err(invalid());
				}
				let n: i64 = number.parse().map_err(|_| invalid())?;
				let seconds = n.checked_mul(multiplier).filter(|s| *s > 0).ok_or_else(invalid)?;
				Value::Duration(i128::from(seconds) * 1_000_000_000)
			}
			Field::After | Field::Before => Value::Instant(instant(&raw).ok_or_else(invalid)?),
			_ => {
				let tokens = text_tokens(&raw);
				if tokens.is_empty() {
					return Err(invalid());
				}
				Value::Text(tokens)
			}
		};
		self.node(Node::Predicate(Predicate { field, raw, value }), span)
	}
}

fn field_name(name: &str, mode: Mode, span: (usize, usize)) -> Result<Field> {
	use Field::*;
	let field = match name.to_ascii_lowercase().as_str() {
		"subreddit" => Subreddit,
		"author" => Author,
		"title" => Title,
		"domain" => Domain,
		"flair" => Flair,
		"post.title" => PostTitle,
		"post.domain" => PostDomain,
		"post.flair" => PostFlair,
		"name" => Name,
		"description" => Description,
		"past" => Past,
		"after" => After,
		"before" => Before,
		_ => return Err(Diagnostic::new("unknown_field", format!("Unknown field: {name}"), span)),
	};
	let valid = match mode {
		Mode::Posts => matches!(field, Subreddit | Author | Title | Domain | Flair | Past | After | Before),
		Mode::Comments => matches!(field, Subreddit | Author | PostTitle | PostDomain | PostFlair | Past | After | Before),
		Mode::Communities => matches!(field, Name | Title | Description),
	};
	if !valid {
		return Err(Diagnostic::new(
			"field_not_available",
			if mode == Mode::Comments && field == Title {
				"Use post.title: for a comment's parent title".into()
			} else {
				format!("{name}: is unavailable in {mode:?}")
			},
			span,
		));
	}
	Ok(field)
}

pub fn domain(value: &str) -> Option<String> {
	if value.is_empty() || value.chars().any(|c| c.is_whitespace() || ":/@?#\\%[]".contains(c)) {
		return None;
	}
	let value = value.strip_suffix('.').unwrap_or(value);
	let host = url::Host::parse(value).ok()?.to_string().to_ascii_lowercase();
	if host.len() > 253
		|| host
			.split('.')
			.any(|p| p.is_empty() || p.len() > 63 || p.starts_with('-') || p.ends_with('-') || !p.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
	{
		return None;
	}
	Some(host.strip_prefix("www.").unwrap_or(&host).to_owned())
}

fn instant(value: &str) -> Option<i128> {
	static FORMAT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}:[0-5]\d(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2}))?$").unwrap());
	if !FORMAT.is_match(value) || value.ends_with("-00:00") {
		return None;
	}
	let date = if value.len() == 10 {
		NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?.and_hms_opt(0, 0, 0)?.and_utc()
	} else {
		DateTime::parse_from_rfc3339(value).ok()?.to_utc()
	};
	Some(i128::from(date.timestamp()) * 1_000_000_000 + i128::from(date.timestamp_subsec_nanos()))
}

pub fn text_tokens(value: &str) -> Vec<String> {
	static TOKENS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{M}\p{N}_]+|[^\p{L}\p{M}\p{N}_\s]").unwrap());
	let normalized: String = value.nfc().default_case_fold().nfc().collect();
	TOKENS.find_iter(&normalized).map(|m| m.as_str().to_owned()).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Truth {
	True,
	False,
	Unknown,
}
impl Truth {
	fn not(self) -> Self {
		match self {
			Self::True => Self::False,
			Self::False => Self::True,
			Self::Unknown => Self::Unknown,
		}
	}
	fn and(self, other: Self) -> Self {
		match (self, other) {
			(Self::False, _) | (_, Self::False) => Self::False,
			(Self::True, Self::True) => Self::True,
			_ => Self::Unknown,
		}
	}
	fn or(self, other: Self) -> Self {
		match (self, other) {
			(Self::True, _) | (_, Self::True) => Self::True,
			(Self::False, Self::False) => Self::False,
			_ => Self::Unknown,
		}
	}
}
#[derive(Clone, Debug)]
pub enum Datum {
	Text(String),
	Absent,
	Unknown,
}
#[derive(Clone, Debug, Default)]
pub struct Record {
	pub fields: HashMap<Field, Datum>,
	pub text: Vec<Datum>,
	pub created: Option<i128>,
}

pub(crate) fn body(value: &str) -> Datum {
	if matches!(
		value.trim().to_ascii_lowercase().as_str(),
		"[deleted]" | "[removed]" | "[removed by reddit]" | "[ removed by moderator ]"
	) {
		Datum::Unknown
	} else {
		Datum::Text(value.into())
	}
}

pub(crate) fn author(value: &str) -> Datum {
	if value == "[deleted]" {
		Datum::Absent
	} else if value.is_empty() {
		Datum::Unknown
	} else {
		Datum::Text(value.into())
	}
}

pub(crate) fn optional(extra: &serde_json::Map<String, serde_json::Value>, key: &str) -> Datum {
	match extra.get(key) {
		Some(serde_json::Value::Null) => Datum::Absent,
		Some(serde_json::Value::String(value)) => Datum::Text(value.clone()),
		_ => Datum::Unknown,
	}
}

pub(crate) fn seconds(value: f64) -> Option<i128> {
	(value.is_finite() && (0.0..253402300800.0).contains(&value)).then_some((value * 1_000_000_000.0).round() as i128)
}

pub(crate) fn post_record(post: &Post) -> Record {
	let mut record = Record::default();
	record.fields.insert(Field::Title, body(&post.title));
	record.fields.insert(Field::Subreddit, Datum::Text(post.subreddit.clone()));
	record.fields.insert(Field::Author, author(&post.author));
	record.fields.insert(Field::Flair, optional(&post.extra, "link_flair_text"));
	record.fields.insert(
		Field::Domain,
		url::Url::parse(&post.url)
			.ok()
			.and_then(|url| url.host_str().map(str::to_owned))
			.map_or(Datum::Unknown, Datum::Text),
	);
	record.text = vec![body(&post.title), body(&post.selftext)];
	record.created = seconds(post.created_utc);
	record
}
impl Expr {
	pub fn evaluate(&self, record: &Record, frozen: i128) -> Truth {
		self.evaluate_cached(record, frozen, &mut HashMap::new())
	}

	fn evaluate_cached<'a>(&self, record: &'a Record, frozen: i128, cache: &mut HashMap<&'a str, Vec<String>>) -> Truth {
		match &self.kind {
			Node::Not(e) => e.evaluate_cached(record, frozen, cache).not(),
			Node::And(a, b) => a.evaluate_cached(record, frozen, cache).and(b.evaluate_cached(record, frozen, cache)),
			Node::Or(a, b) => a.evaluate_cached(record, frozen, cache).or(b.evaluate_cached(record, frozen, cache)),
			Node::Predicate(p) => {
				let boolean = |value| if value { Truth::True } else { Truth::False };
				match &p.value {
					Value::Duration(d) => record.created.map_or(Truth::Unknown, |c| boolean(frozen - d <= c && c <= frozen)),
					Value::Instant(t) => record.created.map_or(Truth::Unknown, |c| boolean(if p.field == Field::After { c >= *t } else { c < *t })),
					_ => {
						let mut matches = |datum: &'a Datum| match datum {
							Datum::Absent => Truth::False,
							Datum::Unknown => Truth::Unknown,
							Datum::Text(s) => boolean(match &p.value {
								Value::Text(tokens) => cache.entry(s.as_str()).or_insert_with(|| text_tokens(s)).windows(tokens.len()).any(|w| w == tokens),
								Value::Exact(v) => {
									if matches!(p.field, Field::Domain | Field::PostDomain) {
										domain(s).as_ref() == Some(v)
									} else {
										s.eq_ignore_ascii_case(v)
									}
								}
								_ => unreachable!(),
							}),
						};
						if p.field == Field::Text {
							record.text.iter().map(matches).fold(Truth::False, Truth::or)
						} else {
							matches(record.fields.get(&p.field).unwrap_or(&Datum::Unknown))
						}
					}
				}
			}
		}
	}
}
