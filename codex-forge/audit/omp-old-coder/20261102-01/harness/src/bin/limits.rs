use btae_eval::dsh::{DshAdapter, DshLimits};
fn prefix() -> String { "{\"type\":\"session\",\"sessionId\":\"".to_string() }
fn body(n: usize) -> String { format!("{}{}\"}}", prefix(), "a".repeat(n)) }
fn feed(hm: &[&[u8]], max_line: usize, max_total: usize, max_events: usize) -> String {
    let lim = DshLimits { max_total_bytes: max_total, max_line_bytes: max_line, max_events };
    let mut a = DshAdapter::new(lim);
    for c in hm { if let Err(e) = a.feed(c) { return format!("ERR {e}"); } }
    match a.finish(Some(0)) { Ok(s) => format!("OK lines={} bytes={}", s.lines_received, s.bytes_received), Err(e) => format!("ERR {e}") }
}
fn main() {
    let base = prefix().len() + 3; // + quotes & brace accounting per body()
    println!("prefix_len={} body(0).len={}", prefix().len(), body(0).len());
    for n in 0..3 {
        let b = body(n);
        println!("body n={n} len={}", b.len());
    }
    // find n so body length == 64 exactly
    for n in 0..70 {
        let b = body(n);
        if b.len() == 64 || b.len() == 65 || b.len() == 66 {
            println!("n={n} len={} limit64 LF: {}", b.len(), feed(&[format!("{b}\n").as_bytes()], 64, 1<<20, 100));
        }
    }
    // CRLF variants at max and max+1 content
    for n in 0..70 {
        let b = body(n);
        if b.len() == 64 || b.len() == 65 {
            println!("n={n} len={} limit64 CRLF: {}", b.len(), feed(&[format!("{b}\r\n").as_bytes()], 64, 1<<20, 100));
        }
    }
    // same lines split so CR and LF are in different chunks
    for n in 0..70 {
        let b = body(n);
        if b.len() == 64 {
            println!("n={n} len=64 split CR|LF: {}", feed(&[format!("{b}\r").as_bytes(), b"\n"], 64, 1<<20, 100));
        }
    }
    // exact total bytes limit
    let s = "{\"type\":\"session\",\"sessionId\":\"s\"}\n";
    println!("total exact: {}", feed(&[s.as_bytes()], 1024, s.len(), 100));
    println!("total minus1: {}", feed(&[s.as_bytes()], 1024, s.len()-1, 100));
    // max events exact
    let two = format!("{}\n{{\"type\":\"thinking\"}}", "{\"type\":\"session\",\"sessionId\":\"s\"}");
    println!("events 2 of limit2: {}", feed(&[two.as_bytes()], 1024, 1<<20, 2));
    println!("events 2 of limit1: {}", feed(&[two.as_bytes()], 1024, 1<<20, 1));
}
