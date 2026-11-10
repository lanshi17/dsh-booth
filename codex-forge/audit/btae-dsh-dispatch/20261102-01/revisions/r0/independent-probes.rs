use btae_eval::dsh::{DshAdapter,DshLimits,UsageStatus,read_dsh};
use serde_json::json;
fn prefix() -> String { lines(&[json!({"type":"session","sessionId":"parent-fixture","cwd":"/fixture"}),json!({"type":"status","phase":"turn_start","turn":1}),json!({"type":"status","phase":"step_start","turn":1,"step":1}),json!({"type":"status","phase":"step_end","turn":1,"step":1,"usage":{"inputTokens":3,"outputTokens":2,"cachedInputTokens":1}}),json!({"type":"status","phase":"step_start","turn":1,"step":2})]) }
fn lines(events:&[serde_json::Value])->String {events.iter().map(|e|format!("{e}\n")).collect()}
fn finish_text()->String {lines(&[json!({"type":"status","phase":"turn_end","turn":1,"reason":{"kind":"completed"}}),json!({"type":"final","text":"done"})])}
#[test]
fn missing_later_usage_must_not_be_promoted_to_complete_field_totals(){let data=prefix()+&lines(&[json!({"type":"status","phase":"step_end","turn":1,"step":2})])+&finish_text();let result=read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).unwrap();assert_eq!(result.usage_status,UsageStatus::Partial);assert_eq!(result.usage.input_tokens,None);assert_eq!(result.usage.output_tokens,None);assert_eq!(result.usage.cached_input_tokens,None);}
#[test]
fn interrupted_next_step_must_make_prior_total_incomplete(){let data=prefix();let result=read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).unwrap();assert!(!result.completed);assert_ne!(result.usage_status,UsageStatus::Complete);assert_eq!(result.usage.total_tokens,None);}
#[test]
fn parser_error_is_terminal_even_if_a_caller_attempts_to_resume(){let mut adapter=DshAdapter::new(DshLimits::default());adapter.feed(prefix().as_bytes()).unwrap();assert!(adapter.feed(b"not-json\n").is_err());let suffix=lines(&[json!({"type":"status","phase":"step_end","turn":1,"step":2,"usage":{"inputTokens":1,"outputTokens":1}})])+&finish_text();assert!(adapter.feed(suffix.as_bytes()).is_err());assert!(adapter.finish(Some(0)).is_err());}
#[test]
fn duplicate_counter_keys_cannot_silently_replace_the_original_value(){let data=prefix()+"{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":2,\"usage\":{\"inputTokens\":99,\"inputTokens\":1,\"outputTokens\":1}}\n"+&finish_text();assert!(read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).is_err());}
#[test]
fn absent_cache_is_unknown_even_if_other_usage_fields_are_complete(){let data=prefix()+&lines(&[json!({"type":"status","phase":"step_end","turn":1,"step":2,"usage":{"inputTokens":1,"outputTokens":1}})])+&finish_text();let result=read_dsh(data.as_bytes(),DshLimits::default(),Some(0)).unwrap();assert_eq!(result.usage_status,UsageStatus::Complete);assert_eq!(result.usage.total_tokens,Some(7));assert_eq!(result.usage.cached_input_tokens,None);}
#[test]
fn real_full_development_transcript_also_parses_without_retaining_contents(){let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../task-events.jsonl");let f=std::fs::File::open(path).unwrap();let result=read_dsh(f,DshLimits::default(),Some(0)).unwrap();assert!(result.completed);assert_eq!(result.usage.input_tokens,Some(92414));assert_eq!(result.usage.output_tokens,Some(45968));assert!(!result.billing_verified);}
