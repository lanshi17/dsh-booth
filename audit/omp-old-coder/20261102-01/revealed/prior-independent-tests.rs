use btae_eval::dsh::{DshAdapter,DshLimits,UsageStatus,read_dsh};
use serde_json::json;
fn prefix() -> String { lines(&[json!({"type":"session","sessionId":"parent-fixture","cwd":"/fixture"}),json!({"type":"status","phase":"turn_start","turn":1}),json!({"type":"status","phase":"step_start","turn":1,"step":1}),json!({"type":"status","phase":"step_end","turn":1,"step":1,"usage":{"inputTokens":3,"outputTokens":2,"cacheReadTokens":1,"totalTokens":6}}),json!({"type":"status","phase":"step_start","turn":1,"step":2})]) }
fn lines(events:&[serde_json::Value])->String {events.iter().map(|e|format!("{e}\n")).collect()}
fn finish_text()->String {lines(&[json!({"type":"status","phase":"turn_end","turn":1,"reason":{"kind":"completed"}}),json!({"type":"final","text":"done"})])}
#[test]
fn missing_later_usage_must_not_be_promoted_to_complete_field_totals(){let data=prefix()+&lines(&[json!({"type":"status","phase":"step_end","turn":1,"step":2})])+&finish_text();let result=read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).unwrap();assert_eq!(result.usage_status,UsageStatus::Partial);assert_eq!(result.usage.input_tokens,None);assert_eq!(result.usage.output_tokens,None);assert_eq!(result.usage.cache_read_tokens,None);}
#[test]
fn interrupted_next_step_must_make_prior_total_incomplete(){let data=prefix();let result=read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).unwrap();assert!(!result.completed);assert_ne!(result.usage_status,UsageStatus::Complete);assert_eq!(result.usage.total_tokens,None);}
#[test]
fn parser_error_is_terminal_even_if_a_caller_attempts_to_resume(){let mut adapter=DshAdapter::new(DshLimits::default());adapter.feed(prefix().as_bytes()).unwrap();assert!(adapter.feed(b"not-json\n").is_err());let suffix=lines(&[json!({"type":"status","phase":"step_end","turn":1,"step":2,"usage":{"inputTokens":1,"outputTokens":1,"totalTokens":2}})])+&finish_text();assert!(adapter.feed(suffix.as_bytes()).is_err());assert!(adapter.finish(Some(0)).is_err());}
#[test]
fn duplicate_counter_keys_cannot_silently_replace_the_original_value(){let data=prefix()+"{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2,\"usage\":{\"inputTokens\":99,\"inputTokens\":1,\"outputTokens\":1}}\n"+&finish_text();assert!(read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).is_err());}
#[test]
fn absent_cache_is_unknown_even_if_other_usage_fields_are_complete(){let data=prefix()+&lines(&[json!({"type":"status","phase":"step_end","turn":1,"step":2,"usage":{"inputTokens":1,"outputTokens":1,"totalTokens":2}})])+&finish_text();let result=read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).unwrap();assert_eq!(result.usage_status,UsageStatus::Complete);assert_eq!(result.usage.total_tokens,Some(8));assert_eq!(result.usage.cache_read_tokens,None);}
#[test]
fn real_full_development_transcript_also_parses_without_retaining_contents(){let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../task-events.jsonl");let f=std::fs::File::open(path).unwrap();let result=read_dsh(f,DshLimits::default(),Some(0)).unwrap();assert!(result.completed);assert_eq!(result.usage.input_tokens,Some(1871486));assert_eq!(result.usage.output_tokens,Some(45968));assert!(!result.billing_verified);}

fn one_step(usage: serde_json::Value) -> String {
    lines(&[
        json!({"type":"session","sessionId":"independent","cwd":"/fixture"}),
        json!({"type":"status","phase":"turn_start","turn":1}),
        json!({"type":"status","phase":"step_start","turn":1,"step":1}),
        json!({"type":"status","phase":"step_end","turn":1,"step":1,"usage":usage}),
    ]) + &finish_text()
}
#[test]
fn disjoint_cache_buckets_and_reasoning_are_counted_exactly_once() {
    let data=one_step(json!({"inputTokens":5,"outputTokens":7,"cacheReadTokens":20,"cacheWriteTokens":3,"reasoningTokens":4,"totalTokens":35}));
    let s=read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).unwrap();
    assert_eq!(s.usage.input_tokens,Some(28));
    assert_eq!(s.usage.output_tokens,Some(7));
    assert_eq!(s.usage.total_tokens,Some(35));
}
#[test]
fn absent_optional_buckets_without_total_cannot_prove_all_input() {
    let data=one_step(json!({"inputTokens":5,"outputTokens":7}));
    let s=read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).unwrap();
    assert_ne!(s.usage_status,UsageStatus::Complete);
    assert_eq!(s.usage.input_tokens,None);
    assert_eq!(s.usage.total_tokens,None);
    assert_eq!(s.observed_usage.output_tokens,Some(7));
    assert!(s.usage_coverage.output_tokens.complete);
}
#[test]
fn explicit_cache_buckets_allow_derivation_without_total() {
    let data=one_step(json!({"inputTokens":5,"outputTokens":7,"cacheReadTokens":20,"cacheWriteTokens":3}));
    let s=read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).unwrap();
    assert_eq!(s.usage_status,UsageStatus::Complete);
    assert_eq!(s.usage.total_tokens,Some(35));
}
#[test]
fn totals_that_hide_known_cache_or_excess_reasoning_are_rejected() {
    for u in [
        json!({"inputTokens":5,"outputTokens":7,"cacheReadTokens":20,"totalTokens":12}),
        json!({"inputTokens":5,"outputTokens":7,"cacheReadTokens":20,"cacheWriteTokens":3,"totalTokens":36}),
        json!({"inputTokens":5,"outputTokens":7,"reasoningTokens":8,"totalTokens":12}),
    ] { assert!(read_dsh(one_step(u).as_bytes(),DshLimits::default(),Some(0)).is_err()); }
}
#[test]
fn failed_limits_poison_the_parser() {
    let mut a=DshAdapter::new(DshLimits{max_line_bytes:2,..DshLimits::default()});
    assert!(a.feed(b"long").is_err());
    assert!(a.feed(b"\n").is_err());
    assert!(a.finish(Some(0)).is_err());
}
#[test]
fn duplicate_nested_opaque_keys_rejected_without_revealing_payloads() {
    let data=prefix()+"{\"type\":\"tool_call\",\"input\":{\"PRIVATE_SENTINEL\":1,\"PRIVATE_SENTINEL\":2}}\n";
    let e=read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).unwrap_err();
    assert!(!e.to_string().contains("PRIVATE_SENTINEL"));
}
#[test]
fn empty_session_identity_is_invalid() {
    let data=one_step(json!({"inputTokens":1,"outputTokens":1,"totalTokens":2})).replace("\"sessionId\":\"independent\"","\"sessionId\":\"\"");
    assert!(read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).is_err());
}
#[test]
fn errors_do_not_leak_offending_event_names() {
    let data=one_step(json!({"inputTokens":1,"outputTokens":1,"totalTokens":2})).replace("\"phase\":\"step_end\"","\"phase\":\"PRIVATE_SENTINEL\"");
    let e=read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).unwrap_err();
    assert!(!e.to_string().contains("PRIVATE_SENTINEL"));
}

#[test]
fn partial_usage_cannot_report_total_below_already_known_buckets() {
    for u in [
        json!({"inputTokens":10,"totalTokens":1}),
        json!({"cacheReadTokens":20,"totalTokens":1}),
        json!({"inputTokens":5,"reasoningTokens":10,"totalTokens":12}),
    ] { assert!(read_dsh(one_step(u).as_bytes(),DshLimits::default(),Some(0)).is_err()); }
}
#[test]
fn timed_out_real_revision_trace_must_not_be_accepted_as_completed() {
    let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../revision-r1-events.jsonl");
    if let Ok(s)=read_dsh(std::fs::File::open(path).unwrap(),DshLimits::default(),Some(124)) {assert!(!s.completed);}
}
