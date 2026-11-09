use btae_eval::digest;
use btae_eval::dsh::DshAdapter;
use btae_eval::dsh::DshError;
use btae_eval::dsh::DshLimits;
use btae_eval::dsh::DshSummary;
use btae_eval::dsh::TurnEndKind;
use btae_eval::dsh::UsageStatus;
use btae_eval::dsh::read_dsh;
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
    assert_eq!(summary.usage.uncached_input_tokens, Some(34_746));
    assert_eq!(summary.usage.cache_read_tokens, None);
    assert_eq!(summary.usage.cache_write_tokens, None);
    assert_eq!(summary.usage.reasoning_tokens, None);
    assert_eq!(summary.observed_usage.uncached_input_tokens, Some(34_746));
    assert_eq!(summary.observed_usage.reported_total_tokens, Some(34_756));
    assert_eq!(summary.observed_usage.cache_read_tokens, None);
    assert_eq!(summary.usage_status, UsageStatus::Complete);
    assert_eq!(summary.steps_total, 1);
    assert_eq!(summary.steps_ended, 1);
    assert_eq!(summary.steps_complete, 1);
    assert_eq!(summary.usage_coverage.input_tokens.observed_steps, 1);
    assert!(summary.usage_coverage.input_tokens.complete);
    assert_eq!(summary.usage_schema, "dsh-headless-0.2.1-rc.3.usage-v1");
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
fn real_cached_development_fixture_matches_published_totals() {
    let bytes = include_bytes!("fixtures/dsh-live-development-sanitized.jsonl");
    let mut adapter = DshAdapter::new(DshLimits::default());
    for chunk in bytes.chunks(7) {
        adapter.feed(chunk).expect("fixture chunk");
    }
    let summary = adapter.finish(Some(0)).expect("fixture");
    assert!(summary.completed);
    assert!(summary.completion_reasons.is_empty());
    assert_eq!(summary.lines_received, 141);
    assert_eq!(summary.total_events(), 141);
    assert_eq!(summary.steps_total, 23);
    assert_eq!(summary.steps_ended, 23);
    assert_eq!(summary.steps_complete, 23);
    assert_eq!(summary.steps_with_usage, 23);
    assert_eq!(summary.event_counts.turn_start, 1);
    assert_eq!(summary.event_counts.step_start, 23);
    assert_eq!(summary.event_counts.step_end, 23);
    assert_eq!(summary.event_counts.turn_end, 1);
    assert_eq!(summary.event_counts.status, 48);

    assert_eq!(summary.usage.input_tokens, Some(1_871_486));
    assert_eq!(summary.usage.output_tokens, Some(45_968));
    assert_eq!(summary.usage.total_tokens, Some(1_917_454));
    assert_eq!(summary.usage.uncached_input_tokens, Some(92_414));
    assert_eq!(summary.usage.cache_read_tokens, Some(1_779_072));
    assert_eq!(summary.usage.cache_write_tokens, None);
    assert_eq!(summary.usage.reasoning_tokens, None);
    assert_eq!(
        summary.usage.input_tokens.unwrap() + summary.usage.output_tokens.unwrap(),
        summary.usage.total_tokens.unwrap()
    );
    assert_eq!(
        summary.usage.uncached_input_tokens.unwrap() + summary.usage.cache_read_tokens.unwrap(),
        summary.usage.input_tokens.unwrap()
    );
    assert_eq!(
        summary.observed_usage.reported_total_tokens,
        Some(1_917_454)
    );
    assert_eq!(summary.observed_usage.uncached_input_tokens, Some(92_414));
    assert_eq!(summary.observed_usage.cache_read_tokens, Some(1_779_072));
    assert_eq!(summary.observed_usage.cache_write_tokens, None);
    assert_eq!(summary.observed_usage.reasoning_tokens, None);
    assert_eq!(summary.usage_status, UsageStatus::Complete);
    assert!(summary.usage_coverage.input_tokens.complete);
    assert!(summary.usage_coverage.output_tokens.complete);
    assert!(summary.usage_coverage.total_tokens.complete);
    assert!(summary.usage_coverage.uncached_input_tokens.complete);
    assert!(summary.usage_coverage.cache_read_tokens.complete);
    assert!(!summary.usage_coverage.cache_write_tokens.complete);
    assert!(!summary.usage_coverage.reasoning_tokens.complete);
    assert!(!summary.billing_verified);
}

#[test]
fn cache_write_reasoning_and_all_prompt_buckets_are_not_double_counted() {
    let input = format!(
        "{OPEN}\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":100,\"outputTokens\":10,\"totalTokens\":160,\"cacheReadTokens\":40,\"cacheWriteTokens\":10,\"reasoningTokens\":5}}}}\n\
         {CLOSE}"
    );
    let summary = parse_ok(&input);
    assert!(summary.completed);
    assert_eq!(summary.usage.uncached_input_tokens, Some(100));
    assert_eq!(summary.usage.cache_read_tokens, Some(40));
    assert_eq!(summary.usage.cache_write_tokens, Some(10));
    assert_eq!(summary.usage.reasoning_tokens, Some(5));
    assert_eq!(summary.usage.input_tokens, Some(150));
    assert_eq!(summary.usage.output_tokens, Some(10));
    assert_eq!(summary.usage.total_tokens, Some(160));
    assert_eq!(
        summary.usage.input_tokens.unwrap() + summary.usage.output_tokens.unwrap(),
        summary.usage.total_tokens.unwrap()
    );
    assert_eq!(summary.usage_status, UsageStatus::Complete);
    assert!(summary.usage_coverage.cache_write_tokens.complete);
    assert!(summary.usage_coverage.reasoning_tokens.complete);
}

#[test]
fn cache_omitted_in_one_step_is_not_reported_as_a_full_cache_total() {
    let input = format!(
        "{OPEN}\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":100,\"outputTokens\":10,\"totalTokens\":160,\"cacheReadTokens\":40,\"cacheWriteTokens\":10,\"reasoningTokens\":5}}}}\n\
         {{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":2}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2,\"usage\":{{\"inputTokens\":50,\"outputTokens\":5,\"totalTokens\":55}}}}\n\
         {CLOSE}"
    );
    let summary = parse_ok(&input);
    assert!(summary.completed);
    assert_eq!(summary.usage.input_tokens, Some(200));
    assert_eq!(summary.usage.output_tokens, Some(15));
    assert_eq!(summary.usage.total_tokens, Some(215));
    assert_eq!(summary.usage.uncached_input_tokens, Some(150));
    // Step 2 omitted all cache/reasoning detail: no full-transcript claim.
    assert_eq!(summary.usage.cache_read_tokens, None);
    assert_eq!(summary.usage.cache_write_tokens, None);
    assert_eq!(summary.usage.reasoning_tokens, None);
    assert_eq!(summary.observed_usage.cache_read_tokens, Some(40));
    assert_eq!(summary.observed_usage.cache_write_tokens, Some(10));
    assert_eq!(summary.observed_usage.reasoning_tokens, Some(5));
    assert_eq!(summary.usage_status, UsageStatus::Complete);
    assert_eq!(summary.usage_coverage.cache_read_tokens.observed_steps, 1);
    assert_eq!(summary.usage_coverage.cache_read_tokens.total_steps, 2);
    assert!(!summary.usage_coverage.cache_read_tokens.complete);
}

#[test]
fn missing_usage_unfinished_step_and_unknown_usage_keep_totals_none() {
    // Step 1 is complete; step 2 ends without usage. The whole transcript's
    // canonical input/output/total must stay None, but observed subtotals remain.
    let missing = format!(
        "{OPEN}\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":10,\"outputTokens\":2,\"totalTokens\":15,\"cacheReadTokens\":3}}}}\n\
         {{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":2}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2}}\n\
         {CLOSE}"
    );
    let summary = parse_ok(&missing);
    assert!(summary.completed, "CLI completion is separate from usage");
    assert_eq!(summary.usage_status, UsageStatus::Partial);
    assert_eq!(summary.usage.input_tokens, None);
    assert_eq!(summary.usage.output_tokens, None);
    assert_eq!(summary.usage.total_tokens, None);
    assert_eq!(summary.observed_usage.uncached_input_tokens, Some(10));
    assert_eq!(summary.observed_usage.output_tokens, Some(2));
    assert_eq!(summary.observed_usage.reported_total_tokens, Some(15));
    assert_eq!(summary.observed_usage.cache_read_tokens, Some(3));
    assert_eq!(summary.usage_coverage.input_tokens.observed_steps, 1);
    assert_eq!(summary.usage_coverage.input_tokens.total_steps, 2);
    assert!(!summary.usage_coverage.input_tokens.complete);
    assert_eq!(summary.steps_with_usage, 1);
    assert_eq!(summary.steps_without_usage, 1);

    // Step 2 started but EOF arrived before step_end. It counts as an unknown
    // started step for every field's coverage.
    let unfinished = format!(
        "{OPEN}\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":10,\"outputTokens\":2,\"totalTokens\":15,\"cacheReadTokens\":3}}}}\n\
         {{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":2}}\n"
    );
    let summary = parse_ok(&unfinished);
    assert!(!summary.completed);
    assert_eq!(summary.usage_status, UsageStatus::Partial);
    assert_eq!(summary.usage.input_tokens, None);
    assert_eq!(summary.usage.output_tokens, None);
    assert_eq!(summary.usage.total_tokens, None);
    assert_eq!(summary.usage.cache_read_tokens, None);
    assert_eq!(summary.observed_usage.cache_read_tokens, Some(3));
    assert_eq!(summary.steps_total, 2);
    assert_eq!(summary.steps_ended, 1);
    assert_eq!(summary.usage_coverage.started_steps, 2);
    assert_eq!(summary.usage_coverage.ended_steps, 1);
    for coverage in [
        summary.usage_coverage.input_tokens,
        summary.usage_coverage.output_tokens,
        summary.usage_coverage.total_tokens,
        summary.usage_coverage.cache_read_tokens,
    ] {
        assert_eq!(coverage.total_steps, 2);
        assert!(!coverage.complete);
    }
    assert!(summary.turns[0].steps_started >= 2);
    assert!(
        summary
            .completion_reasons
            .iter()
            .any(|r| r == "truncated_open_step")
    );

    // No usage object at all is Unknown, and absent fields are None, never 0.
    let unknown = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1}}\n{CLOSE}"
    );
    let summary = parse_ok(&unknown);
    assert_eq!(summary.usage_status, UsageStatus::Unknown);
    assert_eq!(summary.usage.input_tokens, None);
    assert_eq!(summary.usage.output_tokens, None);
    assert_eq!(summary.usage.total_tokens, None);
    assert_eq!(summary.usage.cache_read_tokens, None);
    assert_eq!(summary.observed_usage.uncached_input_tokens, None);
    assert_eq!(summary.observed_usage.reported_total_tokens, None);
    assert_eq!(summary.steps_without_usage, 1);
}

#[test]
fn cache_total_contradictions_overflow_and_reasoning_are_rejected() {
    let cache_exceeds_prompt = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":10,\"outputTokens\":1,\"totalTokens\":12,\"cacheReadTokens\":5}}}}\n{CLOSE}"
    );
    assert!(matches!(
        parse(&cache_exceeds_prompt),
        Err(DshError::Usage(_))
    ));

    let both_cache_contradiction = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":100,\"outputTokens\":10,\"totalTokens\":170,\"cacheReadTokens\":40,\"cacheWriteTokens\":10}}}}\n{CLOSE}"
    );
    assert!(matches!(
        parse(&both_cache_contradiction),
        Err(DshError::Usage(_))
    ));

    let total_below_output = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":2,\"totalTokens\":1}}}}\n{CLOSE}"
    );
    assert!(matches!(
        parse(&total_below_output),
        Err(DshError::Usage(_))
    ));

    let reasoning_exceeds_output = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":1,\"totalTokens\":2,\"reasoningTokens\":2}}}}\n{CLOSE}"
    );
    assert!(matches!(
        parse(&reasoning_exceeds_output),
        Err(DshError::Usage(_))
    ));

    let aggregate_overflow = format!(
        "{OPEN}\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":18446744073709551615,\"outputTokens\":0,\"totalTokens\":18446744073709551615}}}}\n\
         {{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":2}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2,\"usage\":{{\"inputTokens\":1,\"outputTokens\":0,\"totalTokens\":1}}}}\n\
         {CLOSE}"
    );
    assert!(matches!(
        parse(&aggregate_overflow),
        Err(DshError::Usage(_))
    ));
}

#[test]
fn failures_are_sticky_for_later_feed_and_finish() {
    fn assert_sticky(mut adapter: DshAdapter) {
        assert!(matches!(adapter.feed(b"x"), Err(DshError::Poisoned)));
        assert!(matches!(adapter.finish(Some(0)), Err(DshError::Poisoned)));
    }

    let mut bad_json = DshAdapter::new(DshLimits::default());
    assert!(matches!(
        bad_json.feed(b"{\"type\":\n"),
        Err(DshError::InvalidJson { .. })
    ));
    assert_sticky(bad_json);

    let mut non_utf8 = DshAdapter::new(DshLimits::default());
    assert!(matches!(
        non_utf8.feed(b"{\"type\":\"text\",\"text\":\"\xff\"}\n"),
        Err(DshError::NonUtf8 { .. })
    ));
    assert_sticky(non_utf8);

    let mut line_limit = DshAdapter::new(DshLimits {
        max_total_bytes: 1024,
        max_line_bytes: 16,
        max_events: 10,
    });
    assert!(matches!(
        line_limit.feed(b"12345678901234567"),
        Err(DshError::LineBytesLimit { limit: 16 })
    ));
    assert_sticky(line_limit);

    let mut total_limit = DshAdapter::new(DshLimits {
        max_total_bytes: 5,
        max_line_bytes: 1024,
        max_events: 10,
    });
    assert!(matches!(
        total_limit.feed(b"123456"),
        Err(DshError::TotalBytesLimit { limit: 5 })
    ));
    assert_sticky(total_limit);

    let mut event_limit = DshAdapter::new(DshLimits {
        max_total_bytes: 1024,
        max_line_bytes: 1024,
        max_events: 1,
    });
    assert!(matches!(
        event_limit.feed(format!("{SESSION}{TURN_START}").as_bytes()),
        Err(DshError::EventLimit { limit: 1 })
    ));
    assert_sticky(event_limit);

    let mut usage_error = DshAdapter::new(DshLimits::default());
    let bad_usage = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":10,\"outputTokens\":1,\"totalTokens\":12,\"cacheReadTokens\":5}}}}\n"
    );
    assert!(matches!(
        usage_error.feed(bad_usage.as_bytes()),
        Err(DshError::Usage(_))
    ));
    assert_sticky(usage_error);

    let mut protocol_error = DshAdapter::new(DshLimits::default());
    assert!(matches!(
        protocol_error.feed(STEP_1.as_bytes()),
        Err(DshError::Protocol { .. })
    ));
    assert_sticky(protocol_error);
}

#[test]
fn duplicate_keys_and_alias_conflicts_are_rejected() {
    let top_level = "{\"type\":\"session\",\"type\":\"session\",\"sessionId\":\"s\"}\n";
    let error = parse(top_level).expect_err("duplicate top-level key");
    assert!(matches!(error, DshError::InvalidJson { .. }));
    assert!(error.to_string().contains("duplicate"));

    let status_duplicate =
        "{\"type\":\"status\",\"phase\":\"turn_start\",\"phase\":\"turn_start\",\"turn\":1}\n";
    assert!(matches!(
        parse(status_duplicate),
        Err(DshError::InvalidJson { .. })
    ));

    let usage_duplicate = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":99,\"inputTokens\":1,\"outputTokens\":1}}}}\n{CLOSE}"
    );
    assert!(matches!(
        parse(&usage_duplicate),
        Err(DshError::InvalidJson { .. })
    ));

    let alias_conflict = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"input_tokens\":1,\"outputTokens\":1}}}}\n{CLOSE}"
    );
    assert!(matches!(parse(&alias_conflict), Err(DshError::Usage(_))));

    let nested_duplicate =
        format!("{OPEN}{{\"type\":\"tool_call\",\"input\":{{\"a\":1,\"a\":2}}}}\n{CLOSE}");
    assert!(matches!(
        parse(&nested_duplicate),
        Err(DshError::InvalidJson { .. })
    ));

    let secret_duplicate =
        format!("{OPEN}{{\"type\":\"text\",\"text\":\"SECRET-A\",\"text\":\"SECRET-B\"}}\n{CLOSE}");
    let error = parse(&secret_duplicate).expect_err("duplicate text key");
    let message = error.to_string();
    assert!(!message.contains("SECRET-A"));
    assert!(!message.contains("SECRET-B"));
}

#[test]
fn unknown_legacy_aliases_and_empty_usage_are_rejected() {
    for usage in [
        "{\"inputTokens\":1,\"outputTokens\":1,\"cachedInputTokens\":1}",
        "{\"inputTokens\":1,\"outputTokens\":1,\"cacheReadInputTokens\":1}",
        "{\"inputTokens\":1,\"outputTokens\":1,\"mysteryTokens\":1}",
        "{}",
    ] {
        let input = format!(
            "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{usage}}}\n{CLOSE}"
        );
        assert!(
            matches!(parse(&input), Err(DshError::Usage(_))),
            "usage {usage} must be rejected"
        );
    }

    let unknown_status = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"mystery\":1}}\n{CLOSE}"
    );
    assert!(matches!(
        parse(&unknown_status),
        Err(DshError::Protocol { .. })
    ));

    let unknown_event = format!("{OPEN}{{\"type\":\"mystery\"}}\n");
    assert!(matches!(
        parse(&unknown_event),
        Err(DshError::UnsupportedEvent { .. })
    ));

    let null_usage = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":null}}\n{CLOSE}"
    );
    assert!(matches!(parse(&null_usage), Err(DshError::Protocol { .. })));

    let null_cwd = "{\"type\":\"session\",\"sessionId\":\"s\",\"cwd\":null}\n";
    assert!(matches!(parse(null_cwd), Err(DshError::Protocol { .. })));
}

#[test]
fn non_integer_and_null_usage_fields_are_rejected() {
    for input_tokens in [
        "1.5",
        "1e2",
        "\"1\"",
        "-1",
        "true",
        "null",
        "18446744073709551616",
    ] {
        let usage =
            format!("{{\"inputTokens\":{input_tokens},\"outputTokens\":1,\"totalTokens\":2}}");
        let input = format!(
            "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{usage}}}\n{CLOSE}"
        );
        assert!(
            matches!(parse(&input), Err(DshError::Usage(_))),
            "inputTokens={input_tokens} must be rejected"
        );
    }
}

#[test]
fn reasoning_without_output_stays_partial_and_is_not_added() {
    let input = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":10,\"reasoningTokens\":3}}}}\n{CLOSE}"
    );
    let summary = parse_ok(&input);
    assert_eq!(summary.usage_status, UsageStatus::Partial);
    assert_eq!(summary.usage.input_tokens, None);
    assert_eq!(summary.usage.output_tokens, None);
    assert_eq!(summary.usage.total_tokens, None);
    assert_eq!(summary.usage.reasoning_tokens, Some(3));
    assert_eq!(summary.observed_usage.reasoning_tokens, Some(3));
    assert_eq!(summary.usage.uncached_input_tokens, Some(10));
}

#[test]
fn no_total_with_all_prompt_buckets_is_exact_but_one_missing_bucket_is_not() {
    let all_buckets = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":100,\"outputTokens\":10,\"cacheReadTokens\":40,\"cacheWriteTokens\":10,\"reasoningTokens\":5}}}}\n{CLOSE}"
    );
    let summary = parse_ok(&all_buckets);
    assert_eq!(summary.usage_status, UsageStatus::Complete);
    assert_eq!(summary.usage.input_tokens, Some(150));
    assert_eq!(summary.usage.output_tokens, Some(10));
    assert_eq!(summary.usage.total_tokens, Some(160));
    assert_eq!(summary.usage.cache_read_tokens, Some(40));
    assert_eq!(summary.usage.cache_write_tokens, Some(10));
    assert_eq!(summary.usage.reasoning_tokens, Some(5));

    let one_missing = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":100,\"outputTokens\":10,\"cacheReadTokens\":40}}}}\n{CLOSE}"
    );
    let summary = parse_ok(&one_missing);
    assert_eq!(summary.usage_status, UsageStatus::Partial);
    assert_eq!(summary.usage.input_tokens, None);
    assert_eq!(summary.usage.output_tokens, None);
    assert_eq!(summary.usage.total_tokens, None);
    assert_eq!(summary.observed_usage.cache_read_tokens, Some(40));
    // All started steps reported cache read, so the raw field subtotal is a
    // full-transcript subtotal even though the canonical totals are unknown.
    assert_eq!(summary.usage.cache_read_tokens, Some(40));
    assert!(summary.usage_coverage.cache_read_tokens.complete);
    assert!(!summary.usage_coverage.input_tokens.complete);
}

#[test]
fn session_id_must_be_nonempty_and_valid() {
    for session in [
        "{\"type\":\"session\",\"sessionId\":\"\",\"cwd\":\"/x\"}\n",
        "{\"type\":\"session\",\"sessionId\":\"   \",\"cwd\":\"/x\"}\n",
        "{\"type\":\"session\",\"sessionId\":\"bad\\u0000id\",\"cwd\":\"/x\"}\n",
        "{\"type\":\"session\",\"cwd\":\"/x\"}\n",
    ] {
        assert!(
            matches!(parse(session), Err(DshError::Protocol { .. })),
            "session {session} must be rejected"
        );
    }

    let long_id = "a".repeat(513);
    let session = format!("{{\"type\":\"session\",\"sessionId\":\"{long_id}\"}}\n");
    assert!(matches!(parse(&session), Err(DshError::Protocol { .. })));
}

#[test]
fn crlf_line_limit_excludes_the_cr_and_lf() {
    let line = "{\"type\":\"session\",\"sessionId\":\"s\",\"cwd\":\"c\"}";
    let max = line.len();
    let limits = DshLimits {
        max_total_bytes: 4096,
        max_line_bytes: max,
        max_events: 10,
    };
    let mut adapter = DshAdapter::new(limits);
    adapter
        .feed(format!("{line}\r\n").as_bytes())
        .expect("content of exactly max_line_bytes with CRLF");
    let summary = adapter.finish(Some(0)).expect("summary");
    assert!(summary.session.is_some());
    assert_eq!(summary.bytes_received, (max + 2) as u64);

    let mut too_long = DshAdapter::new(limits);
    assert!(matches!(
        too_long.feed(format!("{line} \r\n").as_bytes()),
        Err(DshError::LineBytesLimit { limit }) if limit == max
    ));
}

#[test]
fn multi_step_usage_is_added_once() {
    let input = format!(
        "{OPEN}\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":100,\"outputTokens\":10,\"totalTokens\":150,\"cacheReadTokens\":40}}}}\n\
         {{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":2}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2,\"usage\":{{\"inputTokens\":50,\"outputTokens\":5,\"totalTokens\":55}}}}\n\
         {CLOSE}"
    );
    let summary = parse_ok(&input);
    assert!(summary.completed);
    assert_eq!(summary.steps_total, 2);
    assert_eq!(summary.steps_with_usage, 2);
    assert_eq!(summary.usage.input_tokens, Some(190));
    assert_eq!(summary.usage.output_tokens, Some(15));
    assert_eq!(summary.usage.total_tokens, Some(205));
    assert_eq!(summary.usage.uncached_input_tokens, Some(150));
    assert_eq!(summary.usage.cache_read_tokens, None);
    assert_eq!(summary.observed_usage.cache_read_tokens, Some(40));
    assert_eq!(summary.usage_status, UsageStatus::Complete);
    assert_eq!(summary.turns[0].steps_ended, 2);
}

#[test]
fn duplicate_step_end_is_rejected() {
    let input = format!(
        "{OPEN}\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":1,\"totalTokens\":2}}}}\n\
         {{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":2}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2,\"usage\":{{\"inputTokens\":2,\"outputTokens\":2,\"totalTokens\":4}}}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2,\"usage\":{{\"inputTokens\":2,\"outputTokens\":2,\"totalTokens\":4}}}}\n\
         {CLOSE}"
    );
    let error = parse(&input).expect_err("duplicate step_end");
    assert!(matches!(error, DshError::Protocol { .. }));
    assert!(error.to_string().contains("duplicate step_end"));
}

#[test]
fn cancellation_failure_and_nonzero_exit_never_complete() {
    let cancelled = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":1,\"totalTokens\":2}}}}\n\
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
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":1,\"totalTokens\":2}}}}\n{CLOSE}"
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
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":3,\"outputTokens\":2,\"totalTokens\":5}}}}\n{TURN_DONE}"
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
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":1,\"totalTokens\":2}}}}\n\
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

    let turn_usage = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"turn_end\",\"turn\":1,\"usage\":{{\"inputTokens\":1}},\"reason\":{{\"kind\":\"completed\"}}}}\n"
    );
    assert!(matches!(parse(&turn_usage), Err(DshError::Protocol { .. })));
}

#[test]
fn bytewise_chunks_split_utf8_and_last_line_may_lack_newline() {
    let mut input = format!(
        "{OPEN}{{\"type\":\"text\",\"text\":\"你好世界\"}}\n\
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":7,\"outputTokens\":3,\"totalTokens\":10}}}}\n\
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
    assert_eq!(summary.usage.input_tokens, Some(7));
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
         {{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"outputTokens\":1,\"totalTokens\":2}}}}\n\
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

#[test]
fn partial_usage_rejects_totals_below_known_token_lower_bounds() {
    for usage in [
        r#"{"inputTokens":10,"totalTokens":1}"#,
        r#"{"cacheReadTokens":20,"totalTokens":1}"#,
        r#"{"inputTokens":5,"reasoningTokens":10,"totalTokens":12}"#,
        r#"{"inputTokens":18446744073709551615,"reasoningTokens":1,"totalTokens":18446744073709551615}"#,
    ] {
        let input = format!(
            "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{usage}}}\n{CLOSE}"
        );
        assert!(matches!(parse(&input), Err(DshError::Usage(_))));
    }
}

#[test]
fn reasoning_equal_to_output_is_a_valid_subset_boundary() {
    let input = format!(
        "{OPEN}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":5,\"cacheReadTokens\":0,\"cacheWriteTokens\":0,\"outputTokens\":5,\"reasoningTokens\":5,\"totalTokens\":10}}}}\n{CLOSE}"
    );
    let summary = parse_ok(&input);
    assert!(summary.completed);
    assert_eq!(summary.usage_status, UsageStatus::Complete);
    assert_eq!(summary.usage.input_tokens, Some(5));
    assert_eq!(summary.usage.output_tokens, Some(5));
    assert_eq!(summary.usage.total_tokens, Some(10));
    assert_eq!(summary.usage.reasoning_tokens, Some(5));
    assert!(summary.usage_coverage.reasoning_tokens.complete);
}

#[test]
fn exact_total_byte_limit_accepts_whole_and_chunked_real_transcripts() {
    let bytes = include_bytes!("fixtures/dsh-live-smoke.jsonl");
    for chunk_size in [1, 2, 7, bytes.len()] {
        let limits = DshLimits {
            max_total_bytes: bytes.len(),
            ..DshLimits::default()
        };
        let mut adapter = DshAdapter::new(limits);
        for chunk in bytes.chunks(chunk_size) {
            adapter
                .feed(chunk)
                .expect("the exact total limit is inclusive");
        }
        let summary = adapter.finish(Some(0)).unwrap();
        assert!(summary.completed);
        assert_eq!(summary.bytes_received, bytes.len() as u64);

        let mut too_small = DshAdapter::new(DshLimits {
            max_total_bytes: bytes.len() - 1,
            ..limits
        });
        let mut error = None;
        for chunk in bytes.chunks(chunk_size) {
            if let Err(received) = too_small.feed(chunk) {
                error = Some(received);
                break;
            }
        }
        assert!(
            matches!(error, Some(DshError::TotalBytesLimit { limit }) if limit == bytes.len() - 1)
        );
        assert!(matches!(too_small.finish(Some(0)), Err(DshError::Poisoned)));
    }
}

#[test]
fn turn_end_with_an_open_step_is_a_terminal_protocol_error() {
    let input = format!("{OPEN}{TURN_DONE}");
    let mut adapter = DshAdapter::new(DshLimits::default());
    let error = adapter.feed(input.as_bytes()).unwrap_err();
    assert!(
        matches!(error, DshError::Protocol { event: 4, message } if message == "turn_end while a step is open")
    );
    assert!(matches!(adapter.feed(b""), Err(DshError::Poisoned)));
    assert!(matches!(adapter.finish(Some(0)), Err(DshError::Poisoned)));
}

#[test]
fn exact_line_byte_limit_accepts_content_before_a_later_lf() {
    let bytes = include_bytes!("fixtures/dsh-live-smoke.jsonl");
    let max_line_bytes = bytes
        .split(|byte| *byte == b'\n')
        .map(<[u8]>::len)
        .max()
        .unwrap();
    for chunk_size in [1, 7, max_line_bytes] {
        let limits = DshLimits {
            max_line_bytes,
            ..DshLimits::default()
        };
        let mut adapter = DshAdapter::new(limits);
        for record in bytes.split_inclusive(|byte| *byte == b'\n') {
            let content = record
                .strip_suffix(b"\n")
                .expect("fixture records end with LF");
            for chunk in content.chunks(chunk_size) {
                adapter
                    .feed(chunk)
                    .expect("exact line content limit is inclusive before LF");
            }
            adapter.feed(b"\n").expect("later LF terminates the record");
        }
        let summary = adapter.finish(Some(0)).unwrap();
        assert!(summary.completed);
        assert_eq!(summary.bytes_received, bytes.len() as u64);
    }
    let mut too_small = DshAdapter::new(DshLimits {
        max_line_bytes: max_line_bytes - 1,
        ..DshLimits::default()
    });
    assert!(
        matches!(too_small.feed(bytes), Err(DshError::LineBytesLimit { limit }) if limit == max_line_bytes - 1)
    );
    assert!(matches!(too_small.feed(b""), Err(DshError::Poisoned)));
    assert!(matches!(too_small.finish(Some(0)), Err(DshError::Poisoned)));
}
