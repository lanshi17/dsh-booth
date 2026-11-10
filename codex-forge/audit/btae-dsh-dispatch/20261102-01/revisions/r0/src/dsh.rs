//! Bounded, offline parser for DSH `--profile headless` JSONL transcripts.
//!
//! This module only labels what the CLI itself reported. It never spawns a
//! process, executes a tool, calls a model, or turns a transcript into an
//! evaluator-authored [`crate::Observation`]. Text, tool arguments and tool
//! results are not retained; only bounded counts and SHA256 digests are kept.

use crate::digest;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::{self, Read};
use thiserror::Error;

/// The only provenance label this adapter can justify.
pub const USAGE_SOURCE: &str = "dsh_cli_self_reported";

/// Hard bounds applied while bytes are consumed, never after buffering them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DshLimits {
    /// All bytes received, including delimiters.
    pub max_total_bytes: usize,
    /// Bytes in one JSONL record, excluding the `\n` delimiter and optional `\r`.
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
    #[error("dsh event {event} is not valid JSON: {message}")]
    InvalidJson { event: u64, message: String },
    #[error("dsh event {event} has an unsupported type")]
    UnsupportedEvent { event: u64 },
    #[error("dsh event {event} violates the transcript protocol: {message}")]
    Protocol { event: u64, message: String },
    #[error("dsh self-reported token usage is invalid: {0}")]
    Usage(String),
}

fn protocol(event: u64, message: &str) -> DshError {
    DshError::Protocol {
        event,
        message: message.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageStatus {
    /// No step reported usage; this is not the same as zero usage.
    Unknown,
    /// At least one step or field is absent.
    Partial,
    /// Every completed step reported input and output tokens.
    Complete,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DshTokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// Derived as input + output, never copied blindly from `totalTokens`.
    pub total_tokens: Option<u64>,
    /// A subset of input; it is never added to input or total.
    pub cached_input_tokens: Option<u64>,
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
    pub schema_version: u32,
    pub usage_source: &'static str,
    /// Exit code 0, `final`, and every opened turn completed.
    pub completed: bool,
    /// Why `completed` is false; empty when it is true.
    pub completion_reasons: Vec<String>,
    pub exit_code: Option<i32>,
    pub final_seen: bool,
    pub session: Option<SessionSummary>,
    pub turns: Vec<TurnSummary>,
    pub usage: DshTokenUsage,
    pub usage_status: UsageStatus,
    pub steps_total: u64,
    pub steps_with_usage: u64,
    pub steps_without_usage: u64,
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
    counts: EventCounts,
    text_hasher: Sha256,
    text_events: u64,
    final_text_sha256: Option<String>,
    usage: DshTokenUsage,
    steps_total: u64,
    steps_with_usage: u64,
    steps_without_usage: u64,
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
            counts: EventCounts::default(),
            text_hasher: Sha256::new(),
            text_events: 0,
            final_text_sha256: None,
            usage: DshTokenUsage::default(),
            steps_total: 0,
            steps_with_usage: 0,
            steps_without_usage: 0,
            steps_complete: 0,
        }
    }

    pub fn limits(&self) -> DshLimits {
        self.limits
    }

    /// Consume a chunk. Limits are enforced before bytes are retained.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), DshError> {
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
    /// input or a limit violation returns an error.
    pub fn finish(mut self, exit_code: Option<i32>) -> Result<DshSummary, DshError> {
        if !self.line.is_empty() {
            self.end_line()?;
        }

        let usage_status = if self.steps_total > 0
            && self.steps_with_usage == self.steps_total
            && self.steps_complete == self.steps_total
        {
            UsageStatus::Complete
        } else if self.steps_with_usage == 0 {
            UsageStatus::Unknown
        } else {
            UsageStatus::Partial
        };

        let mut usage = self.usage;
        // A total is meaningful only when every ended step reported both parts.
        if usage_status == UsageStatus::Complete {
            if let (Some(input), Some(output)) = (usage.input_tokens, usage.output_tokens) {
                usage.total_tokens = Some(
                    input
                        .checked_add(output)
                        .ok_or_else(|| DshError::Usage("input + output total overflow".into()))?,
                );
            }
        }
        if let (Some(cached), Some(input)) = (usage.cached_input_tokens, usage.input_tokens) {
            if cached > input {
                return Err(DshError::Usage(
                    "cached input is not a subset of input".into(),
                ));
            }
        }

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
            schema_version: 1,
            usage_source: USAGE_SOURCE,
            completed,
            completion_reasons: reasons,
            exit_code,
            final_seen: self.final_seen,
            session: self.session,
            turns: self.turns,
            usage,
            usage_status,
            steps_total: self.steps_total,
            steps_with_usage: self.steps_with_usage,
            steps_without_usage: self.steps_without_usage,
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
        if next > self.limits.max_line_bytes {
            return Err(DshError::LineBytesLimit {
                limit: self.limits.max_line_bytes,
            });
        }
        self.line.extend_from_slice(segment);
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), DshError> {
        let mut line = std::mem::take(&mut self.line);
        if line.last() == Some(&b'\r') {
            line.pop();
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
        let value: Value = serde_json::from_str(text).map_err(|error| DshError::InvalidJson {
            event,
            message: error.to_string(),
        })?;
        self.apply_event(event, value)?;
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
                let session: SessionEvent = parse_object(object, event, "session")?;
                self.on_session(event, session)
            }
            "status" => {
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
        self.session = Some(SessionSummary {
            id_sha256: session.session_id.as_deref().map(|s| digest(s.as_bytes())),
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
                self.open_step = Some(OpenStep { turn, step });
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
                open_turn.steps_ended += 1;
                self.open_step = None;
                self.steps_total += 1;
                match usage {
                    Some(raw) => {
                        self.accumulate_usage(raw, event)?;
                        self.steps_with_usage += 1;
                    }
                    None => self.steps_without_usage += 1,
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
                let (steps_started, steps_ended) = (open_turn.steps_started, open_turn.steps_ended);
                let Some(last) = self.turns.last_mut() else {
                    return Err(protocol(event, "turn_end without a recorded turn"));
                };
                last.steps_started = steps_started;
                last.steps_ended = steps_ended;
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
        let sum = match (raw.input_tokens, raw.output_tokens) {
            (Some(input), Some(output)) => Some(input.checked_add(output).ok_or_else(|| {
                DshError::Usage(format!("input + output overflow at event {event}"))
            })?),
            _ => None,
        };
        if let (Some(reported), Some(sum)) = (raw.total_tokens, sum) {
            if reported != sum {
                return Err(DshError::Usage(format!(
                    "reported totalTokens does not equal input + output at event {event}"
                )));
            }
        }
        if let Some(cached) = raw.cached_input_tokens {
            let Some(input) = raw.input_tokens else {
                return Err(DshError::Usage(format!(
                    "cached input without input at event {event}"
                )));
            };
            if cached > input {
                return Err(DshError::Usage(format!(
                    "cached input exceeds input at event {event}"
                )));
            }
        }
        add_field(
            &mut self.usage.input_tokens,
            raw.input_tokens,
            "input tokens",
        )?;
        add_field(
            &mut self.usage.output_tokens,
            raw.output_tokens,
            "output tokens",
        )?;
        add_field(
            &mut self.usage.cached_input_tokens,
            raw.cached_input_tokens,
            "cached input tokens",
        )?;
        if let (Some(input), Some(output)) = (self.usage.input_tokens, self.usage.output_tokens) {
            input
                .checked_add(output)
                .ok_or_else(|| DshError::Usage("input + output total overflow".into()))?;
        }
        if raw.input_tokens.is_some() && raw.output_tokens.is_some() {
            self.steps_complete += 1;
        }
        Ok(())
    }
}

fn add_field(slot: &mut Option<u64>, value: Option<u64>, name: &str) -> Result<(), DshError> {
    if let Some(value) = value {
        let current = slot.unwrap_or(0);
        *slot = Some(
            current
                .checked_add(value)
                .ok_or_else(|| DshError::Usage(format!("{name} overflow")))?,
        );
    }
    Ok(())
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
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
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
    usage: Option<RawUsage>,
    #[serde(default)]
    reason: Option<Reason>,
}

#[derive(Debug, Deserialize)]
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
    #[serde(
        rename = "cachedInputTokens",
        alias = "cacheReadInputTokens",
        alias = "cached_input_tokens",
        alias = "cache_read_input_tokens"
    )]
    cached_input_tokens: Option<u64>,
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
