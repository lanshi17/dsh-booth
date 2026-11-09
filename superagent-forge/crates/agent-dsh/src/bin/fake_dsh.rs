//! Deterministic DSH-shaped CLI used only by forge-agent-dsh tests.
//!
//! `forge-fake-dsh --version`
//! `forge-fake-dsh agent exec [--mode normal|bad-json|stderr|huge|sleep]`
use forge_agent_protocol::ExecutionRequest;
use forge_agent_protocol::WORKER_PROTOCOL_VERSION;
use std::env;
use std::io::Read;
use std::io::Write;
use std::io::{self};
use std::thread;
use std::time::Duration;

fn main() {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("--version") {
        println!("dsh-cli 1.0.0-forge");
        return;
    }
    if args.len() < 2 || args[0] != "agent" || args[1] != "exec" {
        eprintln!("usage: forge-fake-dsh --version | agent exec [--mode ...]");
        std::process::exit(2);
    }
    args.drain(0..2);
    let mode = mode_from_args(&args);

    let mut stdin = String::new();
    io::stdin().read_to_string(&mut stdin).expect("stdin");
    let request: ExecutionRequest = serde_json::from_str(&stdin).unwrap_or_else(|err| {
        eprintln!("bad request: {err}");
        std::process::exit(2);
    });

    match mode.as_str() {
        "sleep" => {
            emit_ready();
            thread::sleep(Duration::from_secs(30));
            emit_done(&request, 0, "slept");
        }
        "bad-json" => {
            println!("not-json-at-all");
            std::process::exit(0);
        }
        "stderr" => {
            eprintln!("dsh noise on stderr");
            emit_ready();
            emit_done(&request, 0, "ok");
        }
        "huge" => {
            emit_ready();
            let pad = "y".repeat(80_000);
            for _ in 0..20 {
                println!(
                    "{{\"dsh_protocol\":{WORKER_PROTOCOL_VERSION},\"event\":\"ready\",\"version\":\"{pad}\"}}"
                );
            }
            emit_done(&request, 0, "huge");
        }
        _ => {
            emit_ready();
            emit_done(&request, 0, &format!("dsh:{}", request.prompt));
        }
    }
}

fn mode_from_args(args: &[String]) -> String {
    let mut mode = "normal".to_owned();
    let mut idx = 0;
    while idx < args.len() {
        if args[idx] == "--mode" {
            if let Some(value) = args.get(idx + 1) {
                mode = value.clone();
                idx += 2;
                continue;
            }
        }
        idx += 1;
    }
    mode
}

fn emit_ready() {
    println!(
        "{{\"dsh_protocol\":{WORKER_PROTOCOL_VERSION},\"event\":\"ready\",\"version\":\"1.0.0-forge\"}}"
    );
    let _ = io::stdout().flush();
}

fn emit_done(request: &ExecutionRequest, code: i32, message: &str) {
    let commit = &request.assignment.workspace.base_commit;
    println!(
        "{{\"dsh_protocol\":{WORKER_PROTOCOL_VERSION},\"event\":\"done\",\"code\":{code},\"message\":{message_json},\"commit\":{commit_json},\"files\":[\"dsh.txt\"]}}",
        message_json = serde_json::to_string(message).unwrap(),
        commit_json = serde_json::to_string(commit).unwrap(),
    );
    let _ = io::stdout().flush();
    std::process::exit(code);
}
