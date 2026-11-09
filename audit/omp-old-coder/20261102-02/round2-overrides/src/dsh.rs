//! Bounded, offline parser for DSH `--profile headless` JSONL transcripts.
//!
//! This module only labels what the CLI itself reported. It never spawns a
//! process, executes a tool, calls a model, or turns a transcript into an
//! evaluator-authored [`crate::Observation`]. Text, tool arguments and tool
//! results are not retained; only bounded counts and SHA256 digests are kept.
//!
//! ## Usage semantics (DSH 0.2.x headless JSONL)
//!
//! `inputTokens` is the **uncached** prompt count. `cacheReadTokens` and
//! `cacheWriteTokens` are additional, non-overlapping prompt buckets, so the
//! canonical prompt count is `uncached + cacheRead + cacheWrite`. `totalTokens`
//! is the canonical prompt plus output (and therefore includes cache buckets);
//! it is not an independently addable counter. `reasoningTokens` is a subset of
//! output and is never added to output or total.
//!
//! A step's usage is the aggregate delta for that step across all attempts.
//! Only `step_end` usage is summed; turn-level usage does not exist in this
//! schema and is rejected. All canonical totals are published only when every
//! started step ended and every step has a complete canonical input/output/
//! total; otherwise the canonical totals stay `None` while independently
//! observed raw field subtotals and their coverage remain auditable.

use crate::digest;
use serde::de::{DeserializeOwned, Error as DeError, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::io::{self, Read};
use thiserror::Error;

/// The only provenance label this adapter can justify.
pub const USAGE_SOURCE: &str = "dsh_cli_self_reported";

/// The exact CLI/usage schema this parser was verified against.
///
/// The fixture set only covers CLI `0.2.1-rc.3` (`--profile headless`). A
/// transcript with unknown event types or unknown fields in validated
/// session/status/usage/text/final records is rejected. Opaque thinking/tool/
/// trace records retain only event counts; their payload fields are not
/// validated. This schema identifier does not validate the producing CLI version.
pub const USAGE_SCHEMA: &str = "dsh-headless-0.2.1-rc.3.usage-v1";

/// Hard bounds applied while bytes are consumed, never after buffering them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DshLimits {
    /// All bytes received, including delimiters.
    pub max_total_bytes: usize,
    /// Bytes in one JSONL record, excluding the trailing `\n` and optional
    /// `\r`. This is enforced while bytes arrive (at most one byte of lookahead
    /// is buffered for a possible CR), not after an unbounded read.
    pub max_line_bytes: usize,
    /// Maximum number of non-empty JSONL records.
    pub max_events: usize,
}

impl Default for DshLimits {
    fn default() -> Self {
        Self {
            max_total_bytes: 16 * 1024 * 1024,
            max_line_bytes: 1024 * 1024,
            max_events: 100_000,
        }
    }
}

#[derive(Debug, Error)]
pub enum DshError {
    #[error("dsh transcript I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("dsh transcript exceeds the {limit}-byte total limit")]
    TotalBytesLimit { limit: usize },
    #[error("dsh transcript line exceeds the {limit}-byte limit")]
    LineBytesLimit { limit: usize },
    #[error("dsh transcript exceeds the {limit}-event limit")]
    EventLimit { limit: usize },
    #[error("dsh event {event} is not valid UTF-8")]
    NonUtf8 { event: u64 },
    /// The message is a fixed category, never the offending payload.
    #[error("dsh event {event} is not valid JSON: {message}")]
    InvalidJson { event: u64, message: String },
    #[error("dsh event {event} has an unsupported type")]
    UnsupportedEvent { event: u64 },
    #[error("dsh event {event} violates the transcript protocol: {message}")]
    Protocol { event: u64, message: String },
    #[error("dsh self-reported token usage is invalid: {0}")]
    Usage(String),
    /// Once `feed` fails, the adapter is terminal: later feeds and `finish`
    /// cannot turn a partially consumed transcript into a successful parse.
    #[error("dsh adapter is terminally failed; no further events can be accepted")]
    Poisoned,
}

fn protocol(event: u64, message: &str) -> DshError {
    DshError::Protocol {
        event,
        message: message.into(),
    }
}

fn reject_null_fields(
    object: &Map<String, Value>,
    fields: &[&str],
    event: u64,
) -> Result<(), DshError> {
    if fields
        .iter()
        .any(|field| matches!(object.get(*field), Some(Value::Null)))
    {
        return Err(protocol(event, "null is not a valid event field value"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageStatus {
    /// No step reported usage; this is not the same as zero usage.
    Unknown,
    /// At least one step or one canonical field is absent.
    Partial,
    /// Every started step ended and every step has canonical input, output and
    /// total. Cache/reasoning detail may still be independently incomplete.
    Complete,
}

/// Canonical token totals plus independently complete raw field subtotals.
///
/// `input_tokens`, `output_tokens` and `total_tokens` are only populated when
/// the whole transcript's canonical coverage is complete. The remaining fields
/// are full-transcript raw subtotals and are populated only when every started
/// step reported that field; otherwise they stay `None`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DshTokenUsage {
    /// All prompt tokens, including cache read/write buckets.
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// Always `input_tokens + output_tokens`; never copied blindly from
    /// `totalTokens`.
    pub total_tokens: Option<u64>,
    /// Raw `inputTokens` subtotal (uncached prompt only).
    pub uncached_input_tokens: Option<u64>,
    /// Raw `cacheReadTokens` subtotal; a prompt bucket, never added again.
    pub cache_read_tokens: Option<u64>,
    /// Raw `cacheWriteTokens` subtotal; a prompt bucket, never added again.
    pub cache_write_tokens: Option<u64>,
    /// Raw `reasoningTokens` subtotal; a subset of output, never added again.
    pub reasoning_tokens: Option<u64>,
}

/// Raw observed sums. They may cover fewer than all started steps and must not
/// be read as canonical full-transcript totals.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DshObservedUsage {
    pub uncached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// Sum of the per-step `totalTokens` values (each is prompt + output).
    pub reported_total_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
}

/// Coverage of one field across all started steps, including an unfinished step
/// that reached `step_start` but never `step_end`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FieldCoverage {
    pub observed_steps: u64,
    pub total_steps: u64,
    pub complete: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DshUsageCoverage {
    pub started_steps: u64,
    pub ended_steps: u64,
    /// Canonical all-input coverage.
    pub input_tokens: FieldCoverage,
    pub uncached_input_tokens: FieldCoverage,
    pub output_tokens: FieldCoverage,
    /// Canonical total coverage.
    pub total_tokens: FieldCoverage,
    pub reported_total_tokens: FieldCoverage,
    pub cache_read_tokens: FieldCoverage,
    pub cache_write_tokens: FieldCoverage,
    pub reasoning_tokens: FieldCoverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnEndKind {
    Completed,
    Failed,
    Cancelled,
    Aborted,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TurnSummary {
    pub turn: u64,
    pub steps_started: u64,
    pub steps_ended: u64,
    pub completed: bool,
    pub end_reason: Option<TurnEndKind>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct EventCounts {
    pub session: u64,
    pub status: u64,
    pub turn_start: u64,
    pub step_start: u64,
    pub step_end: u64,
    pub turn_end: u64,
    pub text: u64,
    pub final_event: u64,
    pub thinking: u64,
    pub tool_call: u64,
    pub tool_result: u64,
    pub trace: u64,
    pub tool: u64,
}

impl EventCounts {
    pub fn total(&self) -> u64 {
        self.session
            + self.status
            + self.text
            + self.final_event
            + self.thinking
            + self.tool_call
            + self.tool_result
            + self.trace
            + self.tool
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionSummary {
    pub id_sha256: Option<String>,
    pub cwd_sha256: Option<String>,
}

/// A parsed CLI transcript, not a [`crate::Observation`] and not proof of
/// correctness, test success, review approval or a provider bill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DshSummary {
    /// Summary envelope version. Additive summary fields keep this stable;
    /// `usage_schema` identifies the verified CLI usage semantics.
    pub schema_version: u32,
    /// Exact verified CLI/usage schema identifier.
    pub usage_schema: &'static str,
    pub usage_source: &'static str,
    /// Exit code 0, `final`, and every opened turn completed.
    pub completed: bool,
    /// Why `completed` is false; empty when it is true.
    pub completion_reasons: Vec<String>,
    pub exit_code: Option<i32>,
    pub final_seen: bool,
    pub session: Option<SessionSummary>,
    pub turns: Vec<TurnSummary>,
    /// Canonical totals plus fully covered raw field subtotals.
    pub usage: DshTokenUsage,
    /// Raw observed sums over whatever steps reported each field.
    pub observed_usage: DshObservedUsage,
    pub usage_status: UsageStatus,
    pub usage_coverage: DshUsageCoverage,
    /// Number of started steps, including an unfinished step.
    pub steps_total: u64,
    pub steps_ended: u64,
    pub steps_with_usage: u64,
    pub steps_without_usage: u64,
    pub steps_complete: u64,
    pub event_counts: EventCounts,
    pub text_events: u64,
    /// SHA256 of the concatenated text event payloads, not the payloads.
    pub text_sha256: Option<String>,
    pub final_text_sha256: Option<String>,
    pub bytes_received: u64,
    pub lines_received: u64,
    pub limits: DshLimits,
    /// Always false: no provider invoice or trusted collector was consulted.
    pub billing_verified: bool,
}

impl DshSummary {
    pub fn total_events(&self) -> u64 {
        self.event_counts.total()
    }
}

#[derive(Debug, Clone, Copy)]
struct OpenTurn {
    turn: u64,
    steps_started: u64,
    steps_ended: u64,
}

#[derive(Debug, Clone, Copy)]
struct OpenStep {
    turn: u64,
    step: u64,
}

#[derive(Debug, Default, Clone, Copy)]
struct FieldAccumulator {
    sum: u64,
    seen: u64,
}

impl FieldAccumulator {
    fn observe(&mut self, value: Option<u64>, name: &str) -> Result<(), DshError> {
        if let Some(value) = value {
            self.sum = self
                .sum
                .checked_add(value)
                .ok_or_else(|| DshError::Usage(format!("{name} overflow")))?;
            self.seen += 1;
        }
        Ok(())
    }

    fn full_total(self, started_steps: u64) -> Option<u64> {
        if started_steps > 0 && self.seen == started_steps {
            Some(self.sum)
        } else {
            None
        }
    }

    fn observed_total(self) -> Option<u64> {
        if self.seen == 0 { None } else { Some(self.sum) }
    }

    fn coverage(self, started_steps: u64) -> FieldCoverage {
        FieldCoverage {
            observed_steps: self.seen,
            total_steps: started_steps,
            complete: started_steps > 0 && self.seen == started_steps,
        }
    }
}

/// Streaming JSONL adapter. `feed` may split at any byte or UTF-8 boundary.
pub struct DshAdapter {
    limits: DshLimits,
    received: u64,
    lines: u64,
    line: Vec<u8>,
    session: Option<SessionSummary>,
    turns: Vec<TurnSummary>,
    open_turn: Option<OpenTurn>,
    open_step: Option<OpenStep>,
    ended_steps: BTreeSet<(u64, u64)>,
    final_seen: bool,
    poisoned: bool,
    counts: EventCounts,
    text_hasher: Sha256,
    text_events: u64,
    final_text_sha256: Option<String>,
    uncached: FieldAccumulator,
    output: FieldAccumulator,
    reported_total: FieldAccumulator,
    cache_read: FieldAccumulator,
    cache_write: FieldAccumulator,
    reasoning: FieldAccumulator,
    canonical_input: FieldAccumulator,
    canonical_total: FieldAccumulator,
    steps_total: u64,
    steps_ended: u64,
    steps_with_usage: u64,
    steps_complete: u64,
}

impl DshAdapter {
    pub fn new(limits: DshLimits) -> Self {
        Self {
            limits,
            received: 0,
            lines: 0,
            line: Vec::new(),
            session: None,
            turns: Vec::new(),
            open_turn: None,
            open_step: None,
            ended_steps: BTreeSet::new(),
            final_seen: false,
            poisoned: false,
            counts: EventCounts::default(),
            text_hasher: Sha256::new(),
            text_events: 0,
            final_text_sha256: None,
            uncached: FieldAccumulator::default(),
            output: FieldAccumulator::default(),
            reported_total: FieldAccumulator::default(),
            cache_read: FieldAccumulator::default(),
            cache_write: FieldAccumulator::default(),
            reasoning: FieldAccumulator::default(),
            canonical_input: FieldAccumulator::default(),
            canonical_total: FieldAccumulator::default(),
            steps_total: 0,
            steps_ended: 0,
            steps_with_usage: 0,
            steps_complete: 0,
        }
    }

    pub fn limits(&self) -> DshLimits {
        self.limits
    }

    /// Consume a chunk. Limits are enforced before bytes are retained. Any
    /// error is sticky: later `feed`/`finish` calls return [`DshError::Poisoned`].
    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), DshError> {
        if self.poisoned {
            return Err(DshError::Poisoned);
        }
        let result = self.feed_inner(bytes);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    fn feed_inner(&mut self, bytes: &[u8]) -> Result<(), DshError> {
        let chunk_len = bytes.len() as u64;
        let total = self
            .received
            .checked_add(chunk_len)
            .ok_or(DshError::TotalBytesLimit {
                limit: self.limits.max_total_bytes,
            })?;
        if total > self.limits.max_total_bytes as u64 {
            return Err(DshError::TotalBytesLimit {
                limit: self.limits.max_total_bytes,
            });
        }
        self.received = total;

        let mut start = 0;
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'\n' {
                self.push_segment(&bytes[start..index])?;
                self.end_line()?;
                start = index + 1;
            }
        }
        if start < bytes.len() {
            self.push_segment(&bytes[start..])?;
        }
        Ok(())
    }

    /// Flush the optional last line without `\n` and classify the transcript.
    /// An incomplete transcript still returns an auditable summary; malformed
    /// input, a sticky earlier failure, or a limit violation returns an error.
    pub fn finish(mut self, exit_code: Option<i32>) -> Result<DshSummary, DshError> {
        if self.poisoned {
            return Err(DshError::Poisoned);
        }
        if !self.line.is_empty() {
            self.end_line()?;
        }

        let usage_status = if self.steps_with_usage == 0 {
            UsageStatus::Unknown
        } else if self.steps_total > 0
            && self.steps_ended == self.steps_total
            && self.open_step.is_none()
            && self.canonical_input.seen == self.steps_total
            && self.output.seen == self.steps_total
            && self.canonical_total.seen == self.steps_total
        {
            UsageStatus::Complete
        } else {
            UsageStatus::Partial
        };

        let (input_tokens, output_tokens, total_tokens) = if usage_status == UsageStatus::Complete {
            let input = self.canonical_input.sum;
            let output = self.output.sum;
            let total = self.canonical_total.sum;
            let derived = input
                .checked_add(output)
                .ok_or_else(|| DshError::Usage("canonical input + output overflow".into()))?;
            if derived != total {
                return Err(DshError::Usage(
                    "canonical total does not equal input + output".into(),
                ));
            }
            (Some(input), Some(output), Some(total))
        } else {
            (None, None, None)
        };

        let started = self.steps_total;
        let usage = DshTokenUsage {
            input_tokens,
            output_tokens,
            total_tokens,
            uncached_input_tokens: self.uncached.full_total(started),
            cache_read_tokens: self.cache_read.full_total(started),
            cache_write_tokens: self.cache_write.full_total(started),
            reasoning_tokens: self.reasoning.full_total(started),
        };
        let observed_usage = DshObservedUsage {
            uncached_input_tokens: self.uncached.observed_total(),
            output_tokens: self.output.observed_total(),
            reported_total_tokens: self.reported_total.observed_total(),
            cache_read_tokens: self.cache_read.observed_total(),
            cache_write_tokens: self.cache_write.observed_total(),
            reasoning_tokens: self.reasoning.observed_total(),
        };
        let usage_coverage = DshUsageCoverage {
            started_steps: started,
            ended_steps: self.steps_ended,
            input_tokens: self.canonical_input.coverage(started),
            uncached_input_tokens: self.uncached.coverage(started),
            output_tokens: self.output.coverage(started),
            total_tokens: self.canonical_total.coverage(started),
            reported_total_tokens: self.reported_total.coverage(started),
            cache_read_tokens: self.cache_read.coverage(started),
            cache_write_tokens: self.cache_write.coverage(started),
            reasoning_tokens: self.reasoning.coverage(started),
        };

        let mut reasons = Vec::new();
        match exit_code {
            Some(0) => {}
            Some(_) => reasons.push("exit_code_nonzero".into()),
            None => reasons.push("exit_code_unknown".into()),
        }
        if self.session.is_none() {
            reasons.push("missing_session".into());
        }
        if self.open_step.is_some() {
            reasons.push("truncated_open_step".into());
        }
        if self.open_turn.is_some() {
            reasons.push("truncated_open_turn".into());
        }
        if !self.final_seen {
            reasons.push("missing_final".into());
        }
        if self.turns.is_empty() {
            reasons.push("no_turns".into());
        } else {
            if self.turns.iter().any(|turn| !turn.completed) {
                reasons.push("incomplete_turn".into());
            }
            if !self.turns.iter().any(|turn| turn.completed) {
                reasons.push("no_completed_turn".into());
            }
        }
        let completed = reasons.is_empty();

        let text_sha256 = if self.text_events == 0 {
            None
        } else {
            Some(format!("{:x}", self.text_hasher.finalize()))
        };
        Ok(DshSummary {
            // Additive summary fields keep the original envelope version;
            // `usage_schema` identifies the corrected CLI usage semantics.
            schema_version: 1,
            usage_schema: USAGE_SCHEMA,
            usage_source: USAGE_SOURCE,
            completed,
            completion_reasons: reasons,
            exit_code,
            final_seen: self.final_seen,
            session: self.session,
            turns: self.turns,
            usage,
            observed_usage,
            usage_status,
            usage_coverage,
            steps_total: started,
            steps_ended: self.steps_ended,
            steps_with_usage: self.steps_with_usage,
            steps_without_usage: started.saturating_sub(self.steps_with_usage),
            steps_complete: self.steps_complete,
            event_counts: self.counts,
            text_events: self.text_events,
            text_sha256,
            final_text_sha256: self.final_text_sha256,
            bytes_received: self.received,
            lines_received: self.lines,
            limits: self.limits,
            billing_verified: false,
        })
    }

    fn push_segment(&mut self, segment: &[u8]) -> Result<(), DshError> {
        let next = self
            .line
            .len()
            .checked_add(segment.len())
            .ok_or(DshError::LineBytesLimit {
                limit: self.limits.max_line_bytes,
            })?;
        // Allow one trailing `\r` that the next byte (`\n`) will strip.
        let hard_limit = self.limits.max_line_bytes.saturating_add(1);
        if next > hard_limit {
            return Err(DshError::LineBytesLimit {
                limit: self.limits.max_line_bytes,
            });
        }
        self.line.extend_from_slice(segment);
        if self.line.len() > self.limits.max_line_bytes && self.line.last() != Some(&b'\r') {
            return Err(DshError::LineBytesLimit {
                limit: self.limits.max_line_bytes,
            });
        }
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), DshError> {
        let mut line = std::mem::take(&mut self.line);
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.len() > self.limits.max_line_bytes {
            return Err(DshError::LineBytesLimit {
                limit: self.limits.max_line_bytes,
            });
        }
        let event = self.lines + 1;
        if line.is_empty() {
            return Err(protocol(event, "empty JSONL record"));
        }
        if self.lines >= self.limits.max_events as u64 {
            return Err(DshError::EventLimit {
                limit: self.limits.max_events,
            });
        }
        let text = std::str::from_utf8(&line).map_err(|_| DshError::NonUtf8 { event })?;
        let strict: StrictValue =
            serde_json::from_str(text).map_err(|error| DshError::InvalidJson {
                event,
                message: json_error_message(&error),
            })?;
        self.apply_event(event, strict.into_value())?;
        self.lines = event;
        Ok(())
    }

    fn apply_event(&mut self, event: u64, value: Value) -> Result<(), DshError> {
        if self.final_seen {
            return Err(protocol(event, "event after final"));
        }
        let Value::Object(mut object) = value else {
            return Err(protocol(event, "event is not a JSON object"));
        };
        let Some(type_value) = object.remove("type") else {
            return Err(protocol(event, "missing event type"));
        };
        let Some(event_type) = type_value.as_str() else {
            return Err(protocol(event, "event type is not a string"));
        };
        match event_type {
            "session" => {
                reject_null_fields(&object, &["sessionId", "cwd"], event)?;
                let session: SessionEvent = parse_object(object, event, "session")?;
                self.on_session(event, session)
            }
            "status" => {
                reject_null_fields(
                    &object,
                    &["phase", "turn", "step", "usage", "reason"],
                    event,
                )?;
                let status: StatusEvent = parse_object(object, event, "status")?;
                self.on_status(event, status)
            }
            "text" => {
                let text: TextEvent = parse_object(object, event, "text")?;
                self.on_text(event, text)
            }
            "final" => {
                let final_event: FinalEvent = parse_object(object, event, "final")?;
                self.on_final(event, final_event)
            }
            "thinking" => {
                self.require_open_turn(event, "thinking event outside a turn")?;
                self.counts.thinking += 1;
                Ok(())
            }
            "tool_call" => {
                self.require_open_turn(event, "tool_call event outside a turn")?;
                self.counts.tool_call += 1;
                Ok(())
            }
            "tool_result" => {
                self.require_open_turn(event, "tool_result event outside a turn")?;
                self.counts.tool_result += 1;
                Ok(())
            }
            "trace" => {
                self.require_open_turn(event, "trace event outside a turn")?;
                self.counts.trace += 1;
                Ok(())
            }
            "tool" => {
                self.require_open_turn(event, "tool event outside a turn")?;
                self.counts.tool += 1;
                Ok(())
            }
            _ => Err(DshError::UnsupportedEvent { event }),
        }
    }

    fn require_open_turn(&self, event: u64, message: &str) -> Result<(), DshError> {
        if self.open_turn.is_none() {
            return Err(protocol(event, message));
        }
        Ok(())
    }

    fn on_session(&mut self, event: u64, session: SessionEvent) -> Result<(), DshError> {
        if event != 1 || self.session.is_some() {
            return Err(protocol(
                event,
                "session must be the first and only session",
            ));
        }
        if !valid_session_id(&session.session_id) {
            return Err(protocol(
                event,
                "sessionId must be non-empty, control-free and at most 512 bytes",
            ));
        }
        self.session = Some(SessionSummary {
            id_sha256: Some(digest(session.session_id.as_bytes())),
            cwd_sha256: session.cwd.as_deref().map(|s| digest(s.as_bytes())),
        });
        self.counts.session += 1;
        Ok(())
    }

    fn on_text(&mut self, event: u64, text: TextEvent) -> Result<(), DshError> {
        if self.open_turn.is_none() {
            return Err(protocol(event, "text event outside a turn"));
        }
        self.text_hasher.update(text.text.as_bytes());
        self.text_events += 1;
        self.counts.text += 1;
        Ok(())
    }

    fn on_final(&mut self, event: u64, final_event: FinalEvent) -> Result<(), DshError> {
        if self.final_seen {
            return Err(protocol(event, "duplicate final event"));
        }
        if self.open_turn.is_some() || self.open_step.is_some() {
            return Err(protocol(event, "final before turn_end"));
        }
        if self.turns.is_empty() {
            return Err(protocol(event, "final without a turn"));
        }
        self.final_seen = true;
        self.final_text_sha256 = Some(digest(final_event.text.as_bytes()));
        self.counts.final_event += 1;
        Ok(())
    }

    fn on_status(&mut self, event: u64, status: StatusEvent) -> Result<(), DshError> {
        let StatusEvent {
            phase,
            turn,
            step,
            usage,
            reason,
        } = status;
        if turn == 0 {
            return Err(protocol(event, "turn number must be positive"));
        }
        match phase {
            Phase::TurnStart => {
                if self.open_turn.is_some() || self.open_step.is_some() {
                    return Err(protocol(event, "nested turn_start"));
                }
                if step.is_some() || usage.is_some() || reason.is_some() {
                    return Err(protocol(event, "turn_start carries step/usage/reason"));
                }
                if turn != self.turns.len() as u64 + 1 {
                    return Err(protocol(event, "turn numbers must start at 1 and increase"));
                }
                self.turns.push(TurnSummary {
                    turn,
                    steps_started: 0,
                    steps_ended: 0,
                    completed: false,
                    end_reason: None,
                });
                self.open_turn = Some(OpenTurn {
                    turn,
                    steps_started: 0,
                    steps_ended: 0,
                });
                self.counts.turn_start += 1;
            }
            Phase::StepStart => {
                if usage.is_some() || reason.is_some() {
                    return Err(protocol(event, "step_start carries usage/reason"));
                }
                let Some(step) = step else {
                    return Err(protocol(event, "step_start without step"));
                };
                if step == 0 {
                    return Err(protocol(event, "step number must be positive"));
                }
                if self.open_step.is_some() {
                    return Err(protocol(event, "nested step_start"));
                }
                let Some(open) = self.open_turn.as_mut() else {
                    return Err(protocol(event, "step_start without turn_start"));
                };
                if open.turn != turn {
                    return Err(protocol(event, "step_start turn does not match open turn"));
                }
                if step != open.steps_started + 1 {
                    return Err(protocol(event, "step numbers must start at 1 and increase"));
                }
                open.steps_started += 1;
                if let Some(last) = self.turns.last_mut() {
                    last.steps_started = open.steps_started;
                }
                self.open_step = Some(OpenStep { turn, step });
                self.steps_total += 1;
                self.counts.step_start += 1;
            }
            Phase::StepEnd => {
                if reason.is_some() {
                    return Err(protocol(event, "step_end carries reason"));
                }
                let Some(step) = step else {
                    return Err(protocol(event, "step_end without step"));
                };
                if step == 0 {
                    return Err(protocol(event, "step number must be positive"));
                }
                if !self.ended_steps.insert((turn, step)) {
                    return Err(protocol(event, "duplicate step_end for turn/step"));
                }
                let Some(open_step) = self.open_step else {
                    return Err(protocol(event, "step_end without step_start"));
                };
                if open_step.turn != turn || open_step.step != step {
                    return Err(protocol(event, "step_end does not match open step"));
                }
                let Some(open_turn) = self.open_turn.as_mut() else {
                    return Err(protocol(event, "step_end without turn_start"));
                };
                if open_turn.turn != turn {
                    return Err(protocol(event, "step_end turn does not match open turn"));
                }
                let step_usage = match usage {
                    Some(value) => Some(parse_usage(value, event)?),
                    None => None,
                };
                open_turn.steps_ended += 1;
                if let Some(last) = self.turns.last_mut() {
                    last.steps_ended = open_turn.steps_ended;
                }
                self.open_step = None;
                self.steps_ended += 1;
                if let Some(raw) = step_usage {
                    self.steps_with_usage += 1;
                    self.accumulate_usage(raw, event)?;
                }
                self.counts.step_end += 1;
            }
            Phase::TurnEnd => {
                if step.is_some() || usage.is_some() {
                    return Err(protocol(event, "turn_end carries step/usage"));
                }
                let Some(reason) = reason else {
                    return Err(protocol(event, "turn_end without reason"));
                };
                if reason.kind.is_empty() {
                    return Err(protocol(event, "turn_end reason is empty"));
                }
                if self.open_step.is_some() {
                    return Err(protocol(event, "turn_end while a step is open"));
                }
                let Some(open_turn) = self.open_turn.as_ref() else {
                    return Err(protocol(event, "turn_end without turn_start"));
                };
                if open_turn.turn != turn {
                    return Err(protocol(event, "turn_end does not match open turn"));
                }
                let end_reason = classify_turn_end(&reason.kind);
                let Some(last) = self.turns.last_mut() else {
                    return Err(protocol(event, "turn_end without a recorded turn"));
                };
                last.steps_started = open_turn.steps_started;
                last.steps_ended = open_turn.steps_ended;
                last.completed = end_reason == TurnEndKind::Completed;
                last.end_reason = Some(end_reason);
                self.open_turn = None;
                self.counts.turn_end += 1;
            }
        }
        self.counts.status += 1;
        Ok(())
    }

    fn accumulate_usage(&mut self, raw: RawUsage, event: u64) -> Result<(), DshError> {
        let step = compute_step_usage(raw, event)?;
        self.uncached
            .observe(step.uncached_input, "uncached input")?;
        self.output.observe(step.output, "output")?;
        self.reported_total
            .observe(step.reported_total, "reported total")?;
        self.cache_read.observe(step.cache_read, "cache read")?;
        self.cache_write.observe(step.cache_write, "cache write")?;
        self.reasoning.observe(step.reasoning, "reasoning")?;
        self.canonical_input
            .observe(step.canonical_input, "canonical input")?;
        self.canonical_total
            .observe(step.canonical_total, "canonical total")?;
        if step.canonical_input.is_some() && step.output.is_some() && step.canonical_total.is_some()
        {
            self.steps_complete += 1;
        }
        Ok(())
    }
}

fn valid_session_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 512 && !id.chars().any(|c| c.is_control())
}

fn add_checked(a: u64, b: u64, what: &str) -> Result<u64, DshError> {
    a.checked_add(b)
        .ok_or_else(|| DshError::Usage(format!("{what} overflow")))
}

#[derive(Debug, Clone, Copy)]
struct StepUsage {
    uncached_input: Option<u64>,
    output: Option<u64>,
    reported_total: Option<u64>,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
    reasoning: Option<u64>,
    canonical_input: Option<u64>,
    canonical_total: Option<u64>,
}

fn compute_step_usage(raw: RawUsage, event: u64) -> Result<StepUsage, DshError> {
    // A partial usage object may report reasoning without output. It stays an
    // observed raw field and makes the step partial; the subset relation is
    // only checkable when output is present.
    if let (Some(reasoning), Some(output)) = (raw.reasoning_tokens, raw.output_tokens) {
        if reasoning > output {
            return Err(DshError::Usage(format!(
                "reasoningTokens exceeds outputTokens at event {event}"
            )));
        }
    }

    let mut known_prompt = 0_u64;
    let mut any_prompt_bucket = false;
    for value in [
        raw.input_tokens,
        raw.cache_read_tokens,
        raw.cache_write_tokens,
    ]
    .into_iter()
    .flatten()
    {
        known_prompt = add_checked(known_prompt, value, "known prompt buckets")?;
        any_prompt_bucket = true;
    }
    let all_prompt_buckets = raw.input_tokens.is_some()
        && raw.cache_read_tokens.is_some()
        && raw.cache_write_tokens.is_some();

    // Missing fields do not erase contradictions among fields already known.
    // Reasoning is also a lower bound on output when output is absent.
    if let Some(total) = raw.total_tokens {
        let known_output = raw.output_tokens.or(raw.reasoning_tokens).unwrap_or(0);
        let lower_bound = add_checked(known_prompt, known_output, "known token lower bound")?;
        if total < lower_bound {
            return Err(DshError::Usage(format!(
                "totalTokens is smaller than known token buckets at event {event}"
            )));
        }
    }

    let canonical_from_total = match (raw.total_tokens, raw.output_tokens) {
        (Some(total), Some(output)) => {
            if total < output {
                return Err(DshError::Usage(format!(
                    "totalTokens is smaller than outputTokens at event {event}"
                )));
            }
            let input = total - output;
            if any_prompt_bucket && input < known_prompt {
                return Err(DshError::Usage(format!(
                    "totalTokens - outputTokens is smaller than known prompt buckets at event {event}"
                )));
            }
            if all_prompt_buckets && input != known_prompt {
                return Err(DshError::Usage(format!(
                    "totalTokens does not equal input + cache buckets + output at event {event}"
                )));
            }
            Some(input)
        }
        _ => None,
    };

    let buckets_sum = if all_prompt_buckets {
        Some(known_prompt)
    } else {
        None
    };
    let canonical_input = canonical_from_total.or(buckets_sum);
    let canonical_total = match (canonical_input, raw.output_tokens) {
        (Some(input), Some(output)) => Some(add_checked(input, output, "canonical total")?),
        _ => None,
    };
    if let (Some(reported), Some(computed)) = (raw.total_tokens, canonical_total) {
        if reported != computed {
            return Err(DshError::Usage(format!(
                "reported totalTokens is inconsistent at event {event}"
            )));
        }
    }

    Ok(StepUsage {
        uncached_input: raw.input_tokens,
        output: raw.output_tokens,
        reported_total: raw.total_tokens,
        cache_read: raw.cache_read_tokens,
        cache_write: raw.cache_write_tokens,
        reasoning: raw.reasoning_tokens,
        canonical_input,
        canonical_total,
    })
}

fn classify_turn_end(kind: &str) -> TurnEndKind {
    match kind {
        "completed" => TurnEndKind::Completed,
        "failed" => TurnEndKind::Failed,
        "cancelled" | "canceled" => TurnEndKind::Cancelled,
        "aborted" => TurnEndKind::Aborted,
        _ => TurnEndKind::Other,
    }
}

/// Read a complete stream with the same streaming limits as [`DshAdapter`].
pub fn read_dsh<R: Read>(
    mut reader: R,
    limits: DshLimits,
    exit_code: Option<i32>,
) -> Result<DshSummary, DshError> {
    let mut adapter = DshAdapter::new(limits);
    let mut chunk = [0_u8; 8192];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        adapter.feed(&chunk[..read])?;
    }
    adapter.finish(exit_code)
}

fn parse_object<T: DeserializeOwned>(
    object: Map<String, Value>,
    event: u64,
    kind: &str,
) -> Result<T, DshError> {
    serde_json::from_value(Value::Object(object))
        .map_err(|_| protocol(event, &format!("invalid {kind} event fields")))
}

/// Detect object keys that appeared more than once. `serde_json::Value` alone
/// silently keeps the last duplicate, so the raw line is decoded through this
/// strict tree first, recursively for every nested object.
#[derive(Debug)]
enum StrictValue {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<StrictValue>),
    Object(Vec<(String, StrictValue)>),
}

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StrictVisitor;

        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = StrictValue;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON value")
            }

            fn visit_bool<E: DeError>(self, value: bool) -> Result<Self::Value, E> {
                Ok(StrictValue::Bool(value))
            }

            fn visit_i64<E: DeError>(self, value: i64) -> Result<Self::Value, E> {
                Ok(StrictValue::Number(Number::from(value)))
            }

            fn visit_u64<E: DeError>(self, value: u64) -> Result<Self::Value, E> {
                Ok(StrictValue::Number(Number::from(value)))
            }

            fn visit_f64<E: DeError>(self, value: f64) -> Result<Self::Value, E> {
                Number::from_f64(value)
                    .map(StrictValue::Number)
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E: DeError>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictValue::String(value.to_owned()))
            }

            fn visit_string<E: DeError>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictValue::String(value))
            }

            fn visit_none<E: DeError>(self) -> Result<Self::Value, E> {
                Ok(StrictValue::Null)
            }

            fn visit_unit<E: DeError>(self) -> Result<Self::Value, E> {
                Ok(StrictValue::Null)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                StrictValue::deserialize(deserializer)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<StrictValue>()? {
                    values.push(value);
                }
                Ok(StrictValue::Array(values))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut seen = BTreeSet::new();
                let mut entries = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !seen.insert(key.clone()) {
                        return Err(A::Error::custom("duplicate object key"));
                    }
                    let value = map.next_value::<StrictValue>()?;
                    entries.push((key, value));
                }
                Ok(StrictValue::Object(entries))
            }
        }

        deserializer.deserialize_any(StrictVisitor)
    }
}

impl StrictValue {
    fn into_value(self) -> Value {
        match self {
            StrictValue::Null => Value::Null,
            StrictValue::Bool(value) => Value::Bool(value),
            StrictValue::Number(value) => Value::Number(value),
            StrictValue::String(value) => Value::String(value),
            StrictValue::Array(values) => {
                Value::Array(values.into_iter().map(StrictValue::into_value).collect())
            }
            StrictValue::Object(entries) => {
                let mut object = Map::new();
                for (key, value) in entries {
                    object.insert(key, value.into_value());
                }
                Value::Object(object)
            }
        }
    }
}

fn json_error_message(error: &serde_json::Error) -> String {
    let text = error.to_string();
    if text.contains("duplicate object key") {
        "duplicate object key".into()
    } else if text.contains("recursion limit exceeded") {
        "JSON nesting limit exceeded".into()
    } else {
        "malformed or unsupported JSON value".into()
    }
}

fn parse_usage(value: Value, event: u64) -> Result<RawUsage, DshError> {
    let Value::Object(object) = value else {
        return Err(DshError::Usage(format!(
            "usage at event {event} is not an object"
        )));
    };
    check_usage_alias_conflicts(&object, event)?;
    let raw: RawUsage = serde_json::from_value(Value::Object(object))
        .map_err(|_| DshError::Usage(format!("invalid usage fields at event {event}")))?;
    if raw.is_empty() {
        return Err(DshError::Usage(format!(
            "empty usage object at event {event}"
        )));
    }
    Ok(raw)
}

const USAGE_ALIASES: [(&str, &str); 6] = [
    ("inputTokens", "input_tokens"),
    ("outputTokens", "output_tokens"),
    ("totalTokens", "total_tokens"),
    ("cacheReadTokens", "cache_read_tokens"),
    ("cacheWriteTokens", "cache_write_tokens"),
    ("reasoningTokens", "reasoning_tokens"),
];

fn check_usage_alias_conflicts(object: &Map<String, Value>, event: u64) -> Result<(), DshError> {
    for (camel, snake) in USAGE_ALIASES {
        if object.contains_key(camel) && object.contains_key(snake) {
            return Err(DshError::Usage(format!(
                "ambiguous usage alias at event {event}"
            )));
        }
        for key in [camel, snake] {
            if matches!(object.get(key), Some(Value::Null)) {
                return Err(DshError::Usage(format!(
                    "null usage field at event {event}"
                )));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    TurnStart,
    StepStart,
    StepEnd,
    TurnEnd,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionEvent {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusEvent {
    phase: Phase,
    turn: u64,
    #[serde(default)]
    step: Option<u64>,
    #[serde(default)]
    usage: Option<Value>,
    #[serde(default)]
    reason: Option<Reason>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reason {
    kind: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUsage {
    #[serde(rename = "inputTokens", alias = "input_tokens")]
    input_tokens: Option<u64>,
    #[serde(rename = "outputTokens", alias = "output_tokens")]
    output_tokens: Option<u64>,
    #[serde(rename = "totalTokens", alias = "total_tokens")]
    total_tokens: Option<u64>,
    #[serde(rename = "cacheReadTokens", alias = "cache_read_tokens")]
    cache_read_tokens: Option<u64>,
    #[serde(rename = "cacheWriteTokens", alias = "cache_write_tokens")]
    cache_write_tokens: Option<u64>,
    #[serde(rename = "reasoningTokens", alias = "reasoning_tokens")]
    reasoning_tokens: Option<u64>,
}

impl RawUsage {
    fn is_empty(&self) -> bool {
        self.input_tokens.is_none()
            && self.output_tokens.is_none()
            && self.total_tokens.is_none()
            && self.cache_read_tokens.is_none()
            && self.cache_write_tokens.is_none()
            && self.reasoning_tokens.is_none()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextEvent {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FinalEvent {
    text: String,
}
