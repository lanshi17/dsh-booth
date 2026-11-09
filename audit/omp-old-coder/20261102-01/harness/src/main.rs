use btae_eval::dsh::{read_dsh, DshAdapter, DshLimits};
use std::io::Cursor;

fn parse_line(line: &str) -> String {
    let mut a = DshAdapter::new(DshLimits::default());
    match a.feed(line.as_bytes()).and_then(|_| a.finish(Some(0))) {
        Ok(s) => format!("OK completed={} usage={:?} status={:?} counts_total={} lines={}", s.completed, s.usage, s.usage_status, s.total_events(), s.lines_received),
        Err(e) => format!("ERR {e}"),
    }
}

fn parse_multi(chunks: &[&[u8]]) -> String {
    let mut a = DshAdapter::new(DshLimits::default());
    for c in chunks { if let Err(e) = a.feed(c) { return format!("ERR feed {e}"); } }
    match a.finish(Some(0)) {
        Ok(s) => format!("OK completed={} usage={:?} status={:?} events={} text={:?} final={:?}", s.completed, s.usage, s.usage_status, s.total_events(), s.text_sha256, s.final_text_sha256),
        Err(e) => format!("ERR {e}"),
    }
}

const S: &str = "{\"type\":\"session\",\"sessionId\":\"s\"}\n";
const TS: &str = "{\"type\":\"status\",\"phase\":\"turn_start\",\"turn\":1}\n";
const ST1: &str = "{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":1}\n";
const SE1: &str = "{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{\"inputTokens\":5,\"cacheReadTokens\":20,\"cacheWriteTokens\":3,\"outputTokens\":7,\"totalTokens\":35,\"reasoningTokens\":4}}\n";
const TE: &str = "{\"type\":\"status\",\"phase\":\"turn_end\",\"turn\":1,\"reason\":{\"kind\":\"completed\"}}\n";
const F: &str = "{\"type\":\"final\",\"text\":\"done\"}\n";

fn good() -> String { format!("{S}{TS}{ST1}{SE1}{TE}{F}") }

fn main() {
    // P1 canonical
    println!("P1 canonical: {}", parse_multi(&[good().as_bytes()]));
    // P2 tiny chunks bytewise vs whole
    let g = good();
    println!("P2 whole: {}", parse_multi(&[g.as_bytes()]));
    let bytes: Vec<&[u8]> = g.as_bytes().chunks(1).collect();
    println!("P2 bytewise: {}", parse_multi(&bytes));
    // P3 deep nesting in tool payload
    for depth in [10usize, 127, 200, 2000] {
        let nested = "[".repeat(depth) + &"]".repeat(depth);
        let line = format!("{{\"type\":\"thinking\",\"payload\":{nested}}}\n");
        let before = format!("{S}{TS}");
        let mut a = DshAdapter::new(DshLimits::default());
        let _ = a.feed(before.as_bytes());
        let r = a.feed(line.as_bytes());
        println!("P3 depth {depth}: feed={:?}", r.as_ref().err().map(|e| e.to_string()));
    }
    // P4 duplicate nested key
    println!("P4 dup nested: {}", parse_line("{\"type\":\"session\",\"sessionId\":\"s\",\"x\":{\"k\":1,\"k\":2}}"));
    // P5 unknown status field / unknown usage field / null
    println!("P5 unknown event: {}", parse_line("{\"type\":\"wat\"}"));
    println!("P5 unknown status field: {}", parse_line("{\"type\":\"status\",\"phase\":\"turn_start\",\"turn\":1,\"extra\":1}"));
    println!("P5 unknown usage field: {}", parse_line(&format!("{S}{TS}{ST1}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"cachedInputTokens\":2,\"outputTokens\":1,\"totalTokens\":2}}}}\n")));
    // P6 alias conflict same value
    println!("P6 alias conflict: {}", parse_line(&format!("{S}{TS}{ST1}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{{\"inputTokens\":1,\"input_tokens\":1,\"outputTokens\":1,\"totalTokens\":2}}}}\n")));
    // P7 negative / float / bool / string / huge
    for v in ["-1", "1.5", "true", "\"1\"", "18446744073709551616", "1e400"] {
        let l = format!("{{\"type\":\"status\",\"phase\":\"turn_start\",\"turn\":1,\"x\":{v}}}");
        println!("P7 value {v}: {}", parse_line(&l));
    }
    // P8 overflow of sums across steps
    let mut ov = format!("{S}{TS}");
    for st in 1..=2 { ov.push_str(&format!("{{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":{st}}}\n{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":{st},\"usage\":{{\"inputTokens\":18446744073709551615,\"cacheReadTokens\":0,\"cacheWriteTokens\":0,\"outputTokens\":0,\"totalTokens\":18446744073709551615}}}}\n")); }
    println!("P8 overflow: {}", parse_multi(&[ov.as_bytes()]));
    // P9 total/output mismatch, total<output
    let l = |u: &str| format!("{S}{TS}{ST1}{{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{u}}}\n");
    println!("P9 total<output: {}", parse_multi(&[l("{\"outputTokens\":10,\"totalTokens\":5}").as_bytes()]));
    println!("P9 total mismatch full: {}", parse_multi(&[l("{\"inputTokens\":5,\"cacheReadTokens\":2,\"cacheWriteTokens\":3,\"outputTokens\":3,\"totalTokens\":11}").as_bytes()]));
    println!("P9 reasoning>output: {}", parse_multi(&[l("{\"outputTokens\":3,\"reasoningTokens\":4,\"totalTokens\":5}").as_bytes()]));
    // P10 zero-step completed turn
    println!("P10 zero-step turn: {}", parse_multi(&[format!("{S}{TS}{TE}{F}").as_bytes()]));
    // P11 empty stream
    println!("P11 empty: {}", parse_multi(&[b""]));
    // P12 final before turn_end / event after final
    println!("P12 final early: {}", parse_multi(&[format!("{S}{TS}{F}").as_bytes()]));
    println!("P12 after final: {}", parse_multi(&[format!("{S}{TS}{TE}{F}{TS}").as_bytes()]));
    // P13 line limit CRLF: line of exactly limit content + CRLF
    let lim = DshLimits { max_total_bytes: 1<<20, max_line_bytes: 64, max_events: 100 };
    let mk = |n: usize| { let mut s = String::from("{\"type\":\"session\",\"sessionId\":\""); s.push_str(&"a".repeat(n)); s.push_str("\"}"); s };
    for n in [19usize, 20, 21] {
        let body = mk(n);
        let mut a = DshAdapter::new(lim);
        let r = a.feed(format!("{body}\r\n").as_bytes());
        println!("P13 line len {} CRLF: len={} feed={:?}", body.len(), body.len(), r.err().map(|e| e.to_string()));
    }
    // P14 feed counting: chunk containing multiple lines and a partial exceeding line limit
    // P15 CR-only separators (no LF)
    println!("P15 CR only: {}", parse_multi(&[format!("{S}\r{TS}\r").as_bytes()]));
    // P16 session id boundary lengths
    for n in [511usize, 512, 513] {
        let s = format!("{{\"type\":\"session\",\"sessionId\":\"{}\"}}\n", "a".repeat(n));
        println!("P16 id len {n}: {}", parse_line(&s));
    }
    // P17 full fixture reproduction
    for f in ["../workspace/btae-eval/tests/fixtures/dsh-live-development-sanitized.jsonl", "../workspace/btae-eval/tests/fixtures/dsh-live-smoke.jsonl"] {
        match std::fs::read(f).map_err(|e| e.to_string()).and_then(|b| read_dsh(Cursor::new(b), DshLimits::default(), Some(0)).map_err(|e| e.to_string())) {
            Ok(s) => println!("P17 {f}: completed={} steps={}/{} usage={:?} status={:?} observed={:?} text={:?} final={:?}", s.completed, s.steps_ended, s.steps_total, s.usage, s.usage_status, s.observed_usage, s.text_sha256, s.final_text_sha256),
            Err(e) => println!("P17 {f}: ERR {e}"),
        }
    }
    // P18 counts vs lines
    println!("P18 counts: {}", parse_multi(&[good().as_bytes()]));
    // P19 text hash ambiguity: two different event splits, same concatenation
    let t1 = format!("{S}{TS}{{\"type\":\"text\",\"text\":\"ab\"}}\n{{\"type\":\"text\",\"text\":\"c\"}}\n{TE}{F}");
    let t2 = format!("{S}{TS}{{\"type\":\"text\",\"text\":\"a\"}}\n{{\"type\":\"text\",\"text\":\"bc\"}}\n{TE}{F}");
    println!("P19 text split 1: {}", parse_multi(&[t1.as_bytes()]));
    println!("P19 text split 2: {}", parse_multi(&[t2.as_bytes()]));
    // P20 unknown reason kind
    println!("P20 other reason: {}", parse_multi(&[format!("{S}{TS}{ST1}{SE1}{{\"type\":\"status\",\"phase\":\"turn_end\",\"turn\":1,\"reason\":{{\"kind\":\"weird\"}}}}{F}").as_bytes()]));
    // P21 total bytes boundary exact
    let gb = good();
    let mut lim2 = DshLimits::default(); lim2.max_total_bytes = gb.len();
    let mut a = DshAdapter::new(lim2);
    println!("P21 exact total: feed={:?}", a.feed(gb.as_bytes()).err().map(|e| e.to_string()));
}

// (appended) stdin probe mode is handled by a wrapper binary below
