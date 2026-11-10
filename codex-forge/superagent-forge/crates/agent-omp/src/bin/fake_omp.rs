//! Deterministic OMP-shaped CLI used only by forge-agent-omp tests.
//!
//! `forge-fake-omp --version`
//! `forge-fake-omp run [--mode normal|bad-json|stderr|huge|sleep]`
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
        println!("omp-cli 1.0.0-forge");
        return;
    }
    if args.first().map(String::as_str) != Some("run") {
        eprintln!("usage: forge-fake-omp --version | run [--mode ...]");
        std::process::exit(2);
    }
    args.remove(0);
    let mode = mode_from_args(&args);

    let mut stdin = String::new();
    io::stdin().read_to_string(&mut stdin).expect("stdin");
    let request: ExecutionRequest = serde_json::from_str(&stdin).unwrap_or_else(|err| {
        eprintln!("bad request: {err}");
        std::process::exit(2);
    });

    match mode.as_str() {
        "sleep" => {
            emit_hello();
            thread::sleep(Duration::from_secs(30));
            emit_result(&request, 0, "slept");
        }
        "bad-json" => {
            println!("{{not-json");
            std::process::exit(0);
        }
        "stderr" => {
            eprintln!("omp noise on stderr");
            emit_hello();
            emit_result(&request, 0, "ok");
        }
        "huge" => {
            emit_hello();
            // Exceed default MAX_STDOUT_BYTES (1 MiB) with padding lines.
            let pad = "x".repeat(80_000);
            for _ in 0..20 {
                println!(
                    "{{\"type\":\"hello\",\"version\":{WORKER_PROTOCOL_VERSION},\"backend\":\"omp\",\"cli_version\":\"{pad}\"}}"
                );
            }
            emit_result(&request, 0, "huge");
        }
        _ => {
            emit_hello();
            emit_result(&request, 0, &format!("omp:{}", request.prompt));
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

fn emit_hello() {
    println!(
        "{{\"type\":\"hello\",\"version\":{WORKER_PROTOCOL_VERSION},\"backend\":\"omp\",\"cli_version\":\"1.0.0-forge\"}}"
    );
    let _ = io::stdout().flush();
}

fn emit_result(request: &ExecutionRequest, exit_code: i32, summary: &str) {
    let commit = &request.assignment.workspace.base_commit;
    println!(
        "{{\"type\":\"result\",\"exit_code\":{exit_code},\"summary\":{summary_json},\"head_commit\":{commit_json},\"changed_files\":[\"omp.txt\"]}}",
        summary_json = serde_json::to_string(summary).unwrap(),
        commit_json = serde_json::to_string(commit).unwrap(),
    );
    let _ = io::stdout().flush();
    std::process::exit(exit_code);
}
