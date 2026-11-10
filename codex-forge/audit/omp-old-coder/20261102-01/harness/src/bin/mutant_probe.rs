use btae_eval::dsh::{DshAdapter, DshLimits};
use std::io::{self, Read};
fn main() {
    let mode: u32 = std::env::args().nth(1).unwrap().parse().unwrap();
    let (input, lim, exit): (String, DshLimits, Option<i32>) = match mode {
        1 => ("{\"type\":\"session\",\"sessionId\":\"s\"}\n{\"type\":\"status\",\"phase\":\"turn_start\",\"turn\":1}\n{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":1}\n{\"type\":\"status\",\"phase\":\"step_end\",\"turn\":1,\"step\":1,\"usage\":{\"outputTokens\":5,\"reasoningTokens\":5,\"totalTokens\":10}}\n{\"type\":\"status\",\"phase\":\"turn_end\",\"turn\":1,\"reason\":{\"kind\":\"completed\"}}\n{\"type\":\"final\",\"text\":\"done\"}\n".into(), DshLimits::default(), Some(0)),
        2 => ("{\"type\":\"session\",\"sessionId\":\"s\"}\n".into(), DshLimits { max_total_bytes: 35, max_line_bytes: 1024, max_events: 100 }, Some(0)),
        3 => ("{\"type\":\"session\",\"sessionId\":\"s\"}\n{\"type\":\"status\",\"phase\":\"turn_start\",\"turn\":1}\n{\"type\":\"status\",\"phase\":\"step_start\",\"turn\":1,\"step\":1}\n{\"type\":\"status\",\"phase\":\"turn_end\",\"turn\":1,\"reason\":{\"kind\":\"completed\"}}\n".into(), DshLimits::default(), Some(0)),
        _ => { let mut s=String::new(); io::stdin().read_to_string(&mut s).unwrap(); (s, DshLimits::default(), Some(0)) }
    };
    let mut a = DshAdapter::new(lim);
    match a.feed(input.as_bytes()).and_then(|_| a.finish(exit)) {
        Ok(s) => println!("OK completed={} status={:?} steps={}/{}", s.completed, s.usage_status, s.steps_ended, s.steps_total),
        Err(e) => println!("ERR {e}"),
    }
}
