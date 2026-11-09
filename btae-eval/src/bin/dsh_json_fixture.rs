//! Synthetic JSONL producer for the live process supervisor tests.
use clap::Parser;
use std::io::Read;
use std::io::Write;
use std::io::{self};
use std::process::ExitCode;
use std::thread;
use std::time::Duration;
#[derive(Parser)]
struct Args {
    #[arg(long)]
    scenario: String,
}
fn main() -> ExitCode {
    let args = Args::parse();
    if args.scenario == "stdin-blocked" {
        thread::sleep(Duration::from_secs(2));
        return ExitCode::SUCCESS;
    }
    if args.scenario == "heartbeat" {
        for _ in 0..200 {
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("heartbeat")
                .unwrap();
            f.write_all(b"x").unwrap();
            thread::sleep(Duration::from_millis(10));
        }
        return ExitCode::SUCCESS;
    }
    let mut input = String::new();
    let _ = io::stdin().read_to_string(&mut input);
    match args.scenario.as_str() {
        "incomplete" => {
            println!("{{\"type\":\"session\",\"sessionId\":\"incomplete\"}}");
            ExitCode::SUCCESS
        }
        "failure" => {
            print!(
                "{}",
                include_str!("../../tests/fixtures/dsh-live-smoke.jsonl")
            );
            ExitCode::from(7)
        }
        "timeout" => loop {
            thread::sleep(Duration::from_millis(10));
        },
        "malformed" => {
            println!("not-json");
            io::stdout().flush().unwrap();
            // The supervisor must terminate us; natural exit must not satisfy
            // the protocol-error cleanup test.
            loop {
                thread::sleep(Duration::from_millis(10));
            }
        }
        "env" => {
            if std::env::var_os("HOME").is_some() {
                return ExitCode::from(3);
            }
            print!(
                "{}",
                include_str!("../../tests/fixtures/dsh-live-smoke.jsonl")
            );
            io::stdout().flush().unwrap();
            ExitCode::SUCCESS
        }
        "flood" => {
            let mut e = io::stderr();
            for _ in 0..100 {
                e.write_all(&[b'x'; 4096]).unwrap();
            }
            e.flush().unwrap();
            ExitCode::SUCCESS
        }
        "descendant" => {
            print!(
                "{}",
                include_str!("../../tests/fixtures/dsh-live-smoke.jsonl")
            );
            io::stdout().flush().unwrap();
            // Intentional orphan: this fixture tests supervisor cleanup after the parent exits.
            #[allow(clippy::zombie_processes)]
            let _child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--scenario", "heartbeat"])
                .stdin(std::process::Stdio::null())
                .spawn()
                .unwrap();
            for _ in 0..100 {
                if std::path::Path::new("heartbeat").exists() {
                    break;
                }
                thread::sleep(Duration::from_millis(2));
            }
            ExitCode::SUCCESS
        }
        "task-isolation" => {
            let task: serde_json::Value = serde_json::from_str(&input).unwrap();
            assert_eq!(task["data_kind"], "dsh_task_engineering_only");
            assert!(task["arm_instructions"].as_str().unwrap().len() > 10);
            assert_eq!(std::env::var("HOME").unwrap(), "/tmp");
            assert!(std::env::var_os("USER").is_none());
            assert!(std::fs::read("api.py").is_ok());
            assert_eq!(std::fs::read_dir("/input").unwrap().count(), 1);
            assert!(std::fs::write("/input/task.json", "changed").is_err());
            assert!(std::fs::write("/app/node", "changed").is_err());
            if let Ok(probe) = std::env::var("BTAE_HOST_PROBE") {
                assert!(std::fs::read(probe).is_err());
            }
            if let Ok(address) = std::env::var("BTAE_HOST_ADDRESS") {
                let address = address.parse().unwrap();
                assert!(
                    std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100))
                        .is_err()
                );
            }
            assert_eq!(std::env::var("BTAE_ALLOWED").unwrap(), "yes");
            std::fs::write("worker-edit.txt", "isolated edit").unwrap();
            print!(
                "{}",
                include_str!("../../tests/fixtures/dsh-live-smoke.jsonl")
            );
            io::stdout().flush().unwrap();
            ExitCode::SUCCESS
        }
        "success" => {
            print!(
                "{}",
                include_str!("../../tests/fixtures/dsh-live-smoke.jsonl")
            );
            io::stdout().flush().unwrap();
            ExitCode::SUCCESS
        }
        _ => ExitCode::from(2),
    }
}
