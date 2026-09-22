use super::RankingSignals;
use crate::search::{parse as parse_ql, Diagnostic, Expr as QlExpr, Mode, Record, Truth};

const MAX_BYTES: usize = 512;
const MAX_NODES: usize = 128;
const MAX_DEPTH: usize = 16;

#[derive(Clone, Debug)]
pub(super) struct Program {
	expression: Expr,
	length: usize,
}

#[derive(Clone, Debug)]
enum Expr {
	Number(f64),
	Field(Field),
	Negate(Box<Self>),
	Binary(Binary, Box<Self>, Box<Self>),
	Call(Function, Vec<Self>),
	Ql(QlExpr),
}

#[derive(Clone, Copy, Debug)]
enum Binary {
	Add,
	Subtract,
	Multiply,
	Divide,
}

#[derive(Clone, Copy, Debug)]
enum Field {
	Score,
	UpvoteRatio,
	Comments,
	CreatedUtc,
	FrozenAt,
	AgeSeconds,
	AgeHours,
	SourceRank,
	Awards,
	Crossposts,
	ScoreHidden,
	IsSelf,
	Over18,
	Spoiler,
	Stickied,
	Pinned,
	Locked,
	Archived,
}

#[derive(Clone, Copy, Debug)]
enum Function {
	Abs,
	Sign,
	Sqrt,
	Ln,
	Log1p,
	Log10,
	Min,
	Max,
}

impl Program {
	pub(super) fn parse(source: &str) -> Result<Self, Diagnostic> {
		if source.len() > MAX_BYTES {
			return Err(error("rank_too_complex", "Ranking expression byte limit: 512", (0, source.len())));
		}
		let mut parser = Parser { source, position: 0, nodes: 0 };
		parser.whitespace();
		if parser.position == source.len() {
			return Err(error("invalid_rank", "Enter a ranking expression", (0, source.len())));
		}
		let expression = parser.expression(0)?;
		parser.whitespace();
		if parser.position != source.len() {
			return Err(error(
				"invalid_rank",
				"Unexpected token in ranking expression",
				(parser.position, next_end(source, parser.position)),
			));
		}
		Ok(Self { expression, length: source.len() })
	}

	pub(super) fn evaluate(&self, signals: &RankingSignals, record: &Record, frozen: i128) -> Result<f64, Diagnostic> {
		let value = self.expression.evaluate(signals, record, frozen);
		if value.is_finite() {
			Ok(value)
		} else {
			Err(error(
				"invalid_rank_result",
				"Ranking produced a non-finite value; guard division, logarithms, and square roots",
				(0, self.length),
			))
		}
	}
}

impl Expr {
	fn evaluate(&self, signals: &RankingSignals, record: &Record, frozen: i128) -> f64 {
		match self {
			Self::Number(value) => *value,
			Self::Field(field) => field.value(signals),
			Self::Negate(value) => -value.evaluate(signals, record, frozen),
			Self::Binary(operation, left, right) => {
				let left = left.evaluate(signals, record, frozen);
				let right = right.evaluate(signals, record, frozen);
				match operation {
					Binary::Add => left + right,
					Binary::Subtract => left - right,
					Binary::Multiply => left * right,
					Binary::Divide => left / right,
				}
			}
			Self::Call(function, arguments) => {
				let first = arguments[0].evaluate(signals, record, frozen);
				match function {
					Function::Abs => first.abs(),
					Function::Sign => first.signum(),
					Function::Sqrt => first.sqrt(),
					Function::Ln => first.ln(),
					Function::Log1p => first.ln_1p(),
					Function::Log10 => first.log10(),
					Function::Min => first.min(arguments[1].evaluate(signals, record, frozen)),
					Function::Max => first.max(arguments[1].evaluate(signals, record, frozen)),
				}
			}
			Self::Ql(expression) => boolean(expression.evaluate(record, frozen) == Truth::True),
		}
	}
}

impl Field {
	fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"score" => Self::Score,
			"upvote_ratio" => Self::UpvoteRatio,
			"comments" => Self::Comments,
			"created_utc" => Self::CreatedUtc,
			"frozen_at" => Self::FrozenAt,
			"age_seconds" => Self::AgeSeconds,
			"age_hours" => Self::AgeHours,
			"source_rank" => Self::SourceRank,
			"awards" => Self::Awards,
			"crossposts" => Self::Crossposts,
			"score_hidden" => Self::ScoreHidden,
			"is_self" => Self::IsSelf,
			"over_18" => Self::Over18,
			"spoiler" => Self::Spoiler,
			"stickied" => Self::Stickied,
			"pinned" => Self::Pinned,
			"locked" => Self::Locked,
			"archived" => Self::Archived,
			_ => return None,
		})
	}

	fn value(self, signals: &RankingSignals) -> f64 {
		match self {
			Self::Score => signals.score as f64,
			Self::UpvoteRatio => signals.upvote_ratio,
			Self::Comments => signals.comments as f64,
			Self::CreatedUtc => signals.created_utc,
			Self::FrozenAt => signals.frozen_at,
			Self::AgeSeconds => signals.age_seconds,
			Self::AgeHours => signals.age_hours,
			Self::SourceRank => f64::from(signals.source_rank),
			Self::Awards => signals.awards as f64,
			Self::Crossposts => signals.crossposts as f64,
			Self::ScoreHidden => boolean(signals.score_hidden),
			Self::IsSelf => boolean(signals.is_self),
			Self::Over18 => boolean(signals.over_18),
			Self::Spoiler => boolean(signals.spoiler),
			Self::Stickied => boolean(signals.stickied),
			Self::Pinned => boolean(signals.pinned),
			Self::Locked => boolean(signals.locked),
			Self::Archived => boolean(signals.archived),
		}
	}
}

impl Function {
	fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"abs" => Self::Abs,
			"sign" => Self::Sign,
			"sqrt" => Self::Sqrt,
			"ln" => Self::Ln,
			"log1p" => Self::Log1p,
			"log10" => Self::Log10,
			"min" => Self::Min,
			"max" => Self::Max,
			_ => return None,
		})
	}

	const fn arity(self) -> usize {
		match self {
			Self::Min | Self::Max => 2,
			_ => 1,
		}
	}
}

struct Parser<'a> {
	source: &'a str,
	position: usize,
	nodes: usize,
}

impl Parser<'_> {
	fn expression(&mut self, depth: usize) -> Result<Expr, Diagnostic> {
		let mut expression = self.term(depth + 1)?;
		loop {
			self.whitespace();
			let operation = match self.peek() {
				Some(b'+') => Binary::Add,
				Some(b'-') => Binary::Subtract,
				_ => break,
			};
			self.position += 1;
			let right = self.term(depth + 1)?;
			expression = self.node(Expr::Binary(operation, Box::new(expression), Box::new(right)), depth)?;
		}
		Ok(expression)
	}

	fn term(&mut self, depth: usize) -> Result<Expr, Diagnostic> {
		let mut expression = self.unary(depth + 1)?;
		loop {
			self.whitespace();
			let operation = match self.peek() {
				Some(b'*') => Binary::Multiply,
				Some(b'/') => Binary::Divide,
				_ => break,
			};
			self.position += 1;
			let right = self.unary(depth + 1)?;
			expression = self.node(Expr::Binary(operation, Box::new(expression), Box::new(right)), depth)?;
		}
		Ok(expression)
	}

	fn unary(&mut self, depth: usize) -> Result<Expr, Diagnostic> {
		self.whitespace();
		match self.peek() {
			Some(b'+') => {
				self.position += 1;
				self.unary(depth + 1)
			}
			Some(b'-') => {
				self.position += 1;
				let value = self.unary(depth + 1)?;
				self.node(Expr::Negate(Box::new(value)), depth)
			}
			_ => self.primary(depth + 1),
		}
	}

	fn primary(&mut self, depth: usize) -> Result<Expr, Diagnostic> {
		self.whitespace();
		let start = self.position;
		match self.peek() {
			Some(b'(') => {
				self.position += 1;
				let expression = self.expression(depth + 1)?;
				self.whitespace();
				if self.peek() != Some(b')') {
					return Err(error("invalid_rank", "Close the ranking group", (start, next_end(self.source, start))));
				}
				self.position += 1;
				Ok(expression)
			}
			Some(value) if value.is_ascii_digit() || value == b'.' => {
				let value = self.number()?;
				self.node(Expr::Number(value), depth)
			}
			Some(value) if value.is_ascii_alphabetic() || value == b'_' => self.identifier(depth),
			Some(_) => Err(error("invalid_rank", "Expected a ranking value", (start, next_end(self.source, start)))),
			None => Err(error("invalid_rank", "Expected a ranking value", (start, start))),
		}
	}

	fn identifier(&mut self, depth: usize) -> Result<Expr, Diagnostic> {
		let start = self.position;
		while self.peek().is_some_and(|value| value.is_ascii_alphanumeric() || value == b'_') {
			self.position += 1;
		}
		let name = &self.source[start..self.position];
		self.whitespace();
		if self.peek() != Some(b'(') {
			let field = Field::parse(name).ok_or_else(|| error("unknown_rank_field", format!("Unknown ranking field: {name}"), (start, self.position)))?;
			return self.node(Expr::Field(field), depth);
		}
		if name == "ql" {
			self.position += 1;
			return self.ql(start, depth);
		}
		let function = Function::parse(name).ok_or_else(|| error("unknown_rank_function", format!("Unknown ranking function: {name}"), (start, self.position)))?;
		self.position += 1;
		let mut arguments = Vec::new();
		loop {
			self.whitespace();
			if self.peek() == Some(b')') {
				self.position += 1;
				break;
			}
			arguments.push(self.expression(depth + 1)?);
			self.whitespace();
			match self.peek() {
				Some(b',') => self.position += 1,
				Some(b')') => {
					self.position += 1;
					break;
				}
				_ => {
					return Err(error(
						"invalid_rank",
						"Expected a comma or closing parenthesis",
						(self.position, next_end(self.source, self.position)),
					))
				}
			}
		}
		if arguments.len() != function.arity() {
			return Err(error(
				"invalid_rank",
				format!("{name} expects {} argument{}", function.arity(), if function.arity() == 1 { "" } else { "s" }),
				(start, self.position),
			));
		}
		self.node(Expr::Call(function, arguments), depth)
	}

	fn ql(&mut self, start: usize, depth: usize) -> Result<Expr, Diagnostic> {
		self.whitespace();
		let argument_start = self.position;
		let source = self.string()?;
		self.whitespace();
		if self.peek() != Some(b')') {
			return Err(error(
				"invalid_rank",
				"ql expects one quoted post QL expression",
				(start, next_end(self.source, self.position)),
			));
		}
		self.position += 1;
		let expression =
			parse_ql(&source, Mode::Posts).map_err(|diagnostic| error("invalid_rank_ql", format!("Invalid QL boost: {}", diagnostic.message), (argument_start, self.position)))?;
		self.node(Expr::Ql(expression), depth)
	}

	fn string(&mut self) -> Result<String, Diagnostic> {
		let start = self.position;
		if self.peek() != Some(b'"') {
			return Err(error("invalid_rank", "ql expects a quoted post QL expression", (start, next_end(self.source, start))));
		}
		self.position += 1;
		let mut value = String::new();
		while let Some(byte) = self.peek() {
			match byte {
				b'"' => {
					self.position += 1;
					return Ok(value);
				}
				b'\\' => {
					self.position += 1;
					match self.peek() {
						Some(b'"') => value.push('"'),
						Some(b'\\') => value.push('\\'),
						_ => {
							return Err(error(
								"invalid_rank",
								"Only quoted strings and backslashes can be escaped inside ql",
								(self.position.saturating_sub(1), next_end(self.source, self.position)),
							))
						}
					}
					self.position += 1;
				}
				_ => {
					let character = self.source[self.position..].chars().next().expect("peek found a byte");
					value.push(character);
					self.position += character.len_utf8();
				}
			}
		}
		Err(error("invalid_rank", "Close the ql string", (start, self.source.len())))
	}

	fn number(&mut self) -> Result<f64, Diagnostic> {
		let start = self.position;
		let mut digits = 0;
		while self.peek().is_some_and(|value| value.is_ascii_digit()) {
			digits += 1;
			self.position += 1;
		}
		if self.peek() == Some(b'.') {
			self.position += 1;
			while self.peek().is_some_and(|value| value.is_ascii_digit()) {
				digits += 1;
				self.position += 1;
			}
		}
		if digits == 0 {
			return Err(error("invalid_rank", "Invalid ranking number", (start, self.position)));
		}
		if matches!(self.peek(), Some(b'e' | b'E')) {
			self.position += 1;
			if matches!(self.peek(), Some(b'+' | b'-')) {
				self.position += 1;
			}
			let exponent = self.position;
			while self.peek().is_some_and(|value| value.is_ascii_digit()) {
				self.position += 1;
			}
			if exponent == self.position {
				return Err(error("invalid_rank", "Invalid ranking exponent", (start, self.position)));
			}
		}
		self.source[start..self.position]
			.parse::<f64>()
			.ok()
			.filter(|value| value.is_finite())
			.ok_or_else(|| error("invalid_rank", "Invalid ranking number", (start, self.position)))
	}

	fn node(&mut self, expression: Expr, depth: usize) -> Result<Expr, Diagnostic> {
		self.nodes += 1;
		if self.nodes > MAX_NODES {
			return Err(error("rank_too_complex", "Ranking expression node limit: 128", (0, self.source.len())));
		}
		if depth > MAX_DEPTH {
			return Err(error("rank_too_complex", "Ranking expression nesting limit: 16", (0, self.source.len())));
		}
		Ok(expression)
	}

	fn whitespace(&mut self) {
		while self.peek().is_some_and(|value| value.is_ascii_whitespace()) {
			self.position += 1;
		}
	}

	fn peek(&self) -> Option<u8> {
		self.source.as_bytes().get(self.position).copied()
	}
}

fn boolean(value: bool) -> f64 {
	if value {
		1.0
	} else {
		0.0
	}
}

fn next_end(source: &str, position: usize) -> usize {
	position + source[position..].chars().next().map_or(0, char::len_utf8)
}

fn error(code: &'static str, message: impl Into<String>, span: (usize, usize)) -> Diagnostic {
	Diagnostic::new(code, message, span)
}
