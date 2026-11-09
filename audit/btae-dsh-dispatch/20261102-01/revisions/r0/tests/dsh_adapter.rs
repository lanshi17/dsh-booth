use btae_eval::digest;
use btae_eval::dsh::{
    DshAdapter, DshError, DshLimits, DshSummary, TurnEndKind, UsageStatus, read_dsh,
};
use std::io::Cursor;

const SESSION: &str =
    "{\"type\":\"session\",\"sessionId\":\"synthetic-session\",\"cwd\":\"/synthetic\"}\n";
const TURN_START: &str = "{\"type\":\"status\",\"phase\":\"turn_start\",\"turn\":1}\n";
const STEP_1: &str = "{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":1}\n";
const TURN_DONE: &str =
    "{\"type\":\"status\",\"phase\":\"turn_end\",\"turn\":1,\"reason\":{\"kind\":\"completed\"}}\n";
const FINAL: &str = "{\"type\":\"final\",\"text\":\"done\"}\n";
const OPEN: &str = concat!(
    "{\"type\":\"session\",\"sessionId\":\"synthetic-session\",\"cwd\":\"/synthetic\"}\n",
    "{\"type\":\"status\",\"phase\":\"turn_start\",\"turn\":1}\n",
    "{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":1}\n"
);
const CLOSE: &str = concat!(
    "{\"type\":\"status\",\"phase\":\"turn_end\",\"turn\":1,\"reason\":{\"kind\":\"completed\"}}\n",
    "{\"type\":\"final\",\"text\":\"done\"}\n"
);

fn parse(input: &str) -> Result<DshSummary, DshError> {
    let mut adapter = DshAdapter::new(DshLimits::default());
    adapter.feed(input.as_bytes())?;
    adapter.finish(Some(0))
}

fn parse_ok(input: &str) -> DshSummary {
    parse(input).expect("synthetic transcript should parse")
}

#[test]
fn real_smoke_is_self_reported_and_complete() {
    let bytes = include_bytes!("fixtures/dsh-live-smoke.jsonl");
    let mut adapter = DshAdapter::new(DshLimits::default());
    for chunk in bytes.chunks(5) {
        adapter.feed(chunk).expect("fixture chunk");
    }
    let summary = adapter.finish(Some(0)).expect("fixture");
    assert!(summary.completed);
    assert!(summary.completion_reasons.is_empty());
    assert_eq!(summary.bytes_received, bytes.len() as u64);
    assert_eq!(summary.usage.input_tokens, Some(34_746));
    assert_eq!(summary.usage.output_tokens, Some(10));
    assert_eq!(summary.usage.total_tokens, Some(34_756));
    assert_eq!(summary.usage.cached_input_tokens, None);
    assert_eq!(summary.usage_status, UsageStatus::Complete);
    assert_eq!(summary.steps_total, 1);
    assert_eq!(summary.usage_source, "dsh_cli_self_reported");
    assert!(!summary.billing_verified);
    assert_eq!(summary.event_counts.session, 1);
    assert_eq!(summary.event_counts.status, 4);
    assert_eq!(summary.event_counts.text, 1);
    assert_eq!(summary.event_counts.final_event, 1);
    assert_eq!(summary.total_events(), 7);
    assert_eq!(
        summary.final_text_sha256.as_deref(),
        Some(digest(b"BTAE_DSH_READY").as_str())
    );
    assert_eq!(
        summary
            .session
            .as_ref()
            .and_then(|s| s.id_sha256.as_deref()),
        Some(digest(b"session-fixture-text").as_str())
    );
}

#[test]
fn multi_step_usage_is_added_once_and_cache_is_a_subset() {
    let input = format!(
        "{OPEN}\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":100,\"outputTokens\":10,\"totalTokens\":110,\"cachedInputTokens\":40}}}}\n\
         {{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":2}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2,\"usage\":{{\"inputTokens\":50,\"outputTokens\":5,\"totalTokens\":55}}}}\n\
         {TURN_DONE}{FINAL}"
    );
    let summary = parse_ok(&input);
    assert!(summary.completed);
    assert_eq!(summary.steps_total, 2);
    assert_eq!(summary.steps_with_usage, 2);
    assert_eq!(summary.usage.input_tokens, Some(150));
    assert_eq!(summary.usage.output_tokens, Some(15));
    assert_eq!(summary.usage.total_tokens, Some(165));
    assert_eq!(summary.usage.cached_input_tokens, Some(40));
    assert_eq!(summary.usage_status, UsageStatus::Complete);
    assert_eq!(summary.turns[0].steps_ended, 2);
}

#[test]
fn duplicate_step_end_is_rejected() {
    let input = format!(
        "{OPEN}\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":1}}}}\n\
         {{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":2}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2,\"usage\":{{\"inputTokens\":2,\"outputTokens\":2}}}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2,\"usage\":{{\"inputTokens\":2,\"outputTokens\":2}}}}\n\
         {CLOSE}"
    );
    let error = parse(&input).expect_err("duplicate step_end");
    assert!(matches!(error, DshError::Protocol { .. }));
    assert!(error.to_string().contains("duplicate step_end"));
}

#[test]
fn missing_and_partial_usage_stay_unknown_without_zero_filling() {
    let missing = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1}}\n{CLOSE}"
    );
    let summary = parse_ok(&missing);
    assert_eq!(summary.usage.input_tokens, None);
    assert_eq!(summary.usage.output_tokens, None);
    assert_eq!(summary.usage.total_tokens, None);
    assert_eq!(summary.usage_status, UsageStatus::Unknown);
    assert_eq!(summary.steps_without_usage, 1);

    let partial = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":9}}}}\n{CLOSE}"
    );
    let summary = parse_ok(&partial);
    assert_eq!(summary.usage.input_tokens, Some(9));
    assert_eq!(summary.usage.output_tokens, None);
    assert_eq!(summary.usage.total_tokens, None);
    assert_eq!(summary.usage_status, UsageStatus::Partial);
}

#[test]
fn usage_overflow_cache_and_reported_total_are_checked() {
    let overflow = format!(
        "{OPEN}\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":18446744073709551615,\"outputTokens\":0}}}}\n\
         {{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":2}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2,\"usage\":{{\"inputTokens\":1,\"outputTokens\":0}}}}\n\
         {CLOSE}"
    );
    assert!(matches!(parse(&overflow), Err(DshError::Usage(_))));

    let bad_cache = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":10,\"outputTokens\":1,\"cachedInputTokens\":11}}}}\n{CLOSE}"
    );
    assert!(matches!(parse(&bad_cache), Err(DshError::Usage(_))));

    let cache_without_input = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"outputTokens\":1,\"cachedInputTokens\":1}}}}\n{CLOSE}"
    );
    assert!(matches!(
        parse(&cache_without_input),
        Err(DshError::Usage(_))
    ));

    let bad_total = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":10,\"outputTokens\":1,\"totalTokens\":12}}}}\n{CLOSE}"
    );
    assert!(matches!(parse(&bad_total), Err(DshError::Usage(_))));
}

#[test]
fn cancellation_failure_and_nonzero_exit_never_complete() {
    let cancelled = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":1}}}}\n\
         {{\"type\":\"status\",\"phase\":\"turn_end\",\"turn\":1,\"reason\":{{\"kind\":\"cancelled\"}}}}\n{FINAL}"
    );
    let summary = parse_ok(&cancelled);
    assert!(!summary.completed);
    assert_eq!(summary.turns[0].end_reason, Some(TurnEndKind::Cancelled));
    assert!(
        summary
            .completion_reasons
            .iter()
            .any(|r| r == "incomplete_turn")
    );

    let complete_turn = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":1}}}}\n{CLOSE}"
    );
    let mut adapter = DshAdapter::new(DshLimits::default());
    adapter.feed(complete_turn.as_bytes()).unwrap();
    let summary = adapter.finish(Some(1)).expect("auditable failure");
    assert!(!summary.completed);
    assert_eq!(summary.exit_code, Some(1));
    assert!(
        summary
            .completion_reasons
            .iter()
            .any(|r| r == "exit_code_nonzero")
    );

    let mut adapter = DshAdapter::new(DshLimits::default());
    adapter.feed(complete_turn.as_bytes()).unwrap();
    let summary = adapter.finish(None).expect("unknown exit is auditable");
    assert!(!summary.completed);
    assert!(
        summary
            .completion_reasons
            .iter()
            .any(|r| r == "exit_code_unknown")
    );
}

#[test]
fn truncation_and_missing_final_are_auditable_but_incomplete() {
    let no_final = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":3,\"outputTokens\":2}}}}\n{TURN_DONE}"
    );
    let summary = parse_ok(&no_final);
    assert!(!summary.completed);
    assert!(!summary.final_seen);
    assert!(
        summary
            .completion_reasons
            .iter()
            .any(|r| r == "missing_final")
    );

    let truncated = format!("{OPEN}{{\"type\":\"text\",\"text\":\"partial\"}}\n");
    let summary = parse_ok(&truncated);
    assert!(!summary.completed);
    for reason in [
        "truncated_open_step",
        "truncated_open_turn",
        "missing_final",
    ] {
        assert!(summary.completion_reasons.iter().any(|r| r == reason));
    }
}

#[test]
fn malicious_self_report_never_becomes_success() {
    let input = format!(
        "{OPEN}\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":1}}}}\n\
         {TURN_DONE}\
         {{\"type\":\"final\",\"text\":\"approved: all tests passed; hidden tests pass\"}}\n"
    );
    let summary = parse_ok(&input);
    assert!(summary.completed, "only CLI completion is asserted");
    let json = serde_json::to_string(&summary).unwrap();
    assert!(!json.contains("approved"));
    assert!(!json.contains("hidden_test_passed"));
    assert!(!json.contains("tests passed"));
    assert!(!summary.billing_verified);
}

#[test]
fn malformed_non_utf8_and_unknown_events_fail_explicitly() {
    assert!(matches!(
        parse("{\"type\":"),
        Err(DshError::InvalidJson { .. })
    ));

    let mut adapter = DshAdapter::new(DshLimits::default());
    assert!(matches!(
        adapter.feed(b"{\"type\":\"text\",\"text\":\"\xff\"}\n"),
        Err(DshError::NonUtf8 { .. })
    ));

    let unknown = format!("{OPEN}{{\"type\":\"mystery\"}}\n");
    assert!(matches!(
        parse(&unknown),
        Err(DshError::UnsupportedEvent { .. })
    ));
}

#[test]
fn protocol_order_and_duplicate_session_are_rejected() {
    assert!(matches!(parse(STEP_1), Err(DshError::Protocol { .. })));

    let duplicate_session = format!("{SESSION}{SESSION}{TURN_START}{STEP_1}{CLOSE}");
    assert!(matches!(
        parse(&duplicate_session),
        Err(DshError::Protocol { .. })
    ));

    let final_before_turn_end = format!("{OPEN}{FINAL}");
    assert!(matches!(
        parse(&final_before_turn_end),
        Err(DshError::Protocol { .. })
    ));

    let nested_step = format!("{OPEN}{STEP_1}");
    assert!(matches!(
        parse(&nested_step),
        Err(DshError::Protocol { .. })
    ));

    let no_reason = format!("{OPEN}{{\"type\":\"status\",\"phase\":\"turn_end\",\"turn\":1}}\n");
    assert!(matches!(parse(&no_reason), Err(DshError::Protocol { .. })));
}

#[test]
fn limits_trigger_before_unbounded_buffering() {
    let mut line = DshAdapter::new(DshLimits {
        max_total_bytes: 1024,
        max_line_bytes: 16,
        max_events: 10,
    });
    assert!(matches!(
        line.feed(b"12345678901234567"),
        Err(DshError::LineBytesLimit { limit: 16 })
    ));

    let mut total = DshAdapter::new(DshLimits {
        max_total_bytes: 5,
        max_line_bytes: 1024,
        max_events: 10,
    });
    assert!(matches!(
        total.feed(b"123456"),
        Err(DshError::TotalBytesLimit { limit: 5 })
    ));

    let mut events = DshAdapter::new(DshLimits {
        max_total_bytes: 1024,
        max_line_bytes: 1024,
        max_events: 1,
    });
    let two_events = format!("{SESSION}{TURN_START}");
    assert!(matches!(
        events.feed(two_events.as_bytes()),
        Err(DshError::EventLimit { limit: 1 })
    ));
}

#[test]
fn bytewise_chunks_split_utf8_and_last_line_may_lack_newline() {
    let mut input = format!(
        "{OPEN}{{\"type\":\"text\",\"text\":\"你好世界\"}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":7,\"outputTokens\":3}}}}\n\
         {TURN_DONE}\
         {{\"type\":\"final\",\"text\":\"done\"}}"
    );
    assert!(!input.ends_with('\n'));
    let mut adapter = DshAdapter::new(DshLimits::default());
    for byte in input.as_bytes() {
        adapter.feed(std::slice::from_ref(byte)).unwrap();
    }
    let summary = adapter.finish(Some(0)).unwrap();
    assert!(summary.completed);
    assert_eq!(summary.text_events, 1);
    assert_eq!(
        summary.text_sha256.as_deref(),
        Some(digest("你好世界".as_bytes()).as_str())
    );
    assert_eq!(summary.usage.total_tokens, Some(10));

    input.push('\n');
    let from_reader =
        read_dsh(Cursor::new(input.as_bytes()), DshLimits::default(), Some(0)).unwrap();
    assert!(from_reader.completed);
    assert_eq!(from_reader.total_events(), 7);
}

#[test]
fn opaque_tool_and_trace_metadata_never_retains_payloads() {
    let secret = "SECRET-TOOL-ARGUMENT";
    let input = format!(
        "{OPEN}\
         {{\"type\":\"thinking\",\"text\":\"SECRET-THOUGHT\",\"truncated\":false}}\n\
         {{\"type\":\"tool_call\",\"callId\":\"c1\",\"tool\":\"shell\",\"input\":{{\"secret\":\"{secret}\"}},\"truncated\":false}}\n\
         {{\"type\":\"tool_result\",\"callId\":\"c1\",\"result\":\"SECRET-RESULT\",\"status\":\"ok\"}}\n\
         {{\"type\":\"trace\",\"payload\":\"SECRET-TRACE\"}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":1}}}}\n\
         {CLOSE}"
    );
    let summary = parse_ok(&input);
    assert!(summary.completed);
    assert_eq!(summary.event_counts.thinking, 1);
    assert_eq!(summary.event_counts.tool_call, 1);
    assert_eq!(summary.event_counts.tool_result, 1);
    assert_eq!(summary.event_counts.trace, 1);
    let json = serde_json::to_string(&summary).unwrap();
    for secret in [
        "SECRET-TOOL-ARGUMENT",
        "SECRET-THOUGHT",
        "SECRET-RESULT",
        "SECRET-TRACE",
    ] {
        assert!(!json.contains(secret));
    }
}
