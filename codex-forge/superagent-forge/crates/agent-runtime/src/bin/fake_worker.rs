//! Deterministic worker used only by forge-agent-runtime tests.
//!
//! Modes (argv, never shell):
//! - `--exit <code>`
//! - `--sleep-ms <ms>` then exit 0
//! - `--write <relpath> <contents>` then exit 0
//! - `--write-abs <abspath> <contents>` then exit 0
//! - `--spawn-orphan-ms <ms>` spawn a grandchild that sleeps (same process group)
//! - `--env-get <name>` print env value or exit 3 if absent
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

fn main() {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--exit") => {
            let code = args.next().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
            std::process::exit(code);
        }
        Some("--sleep-ms") => {
            let ms = args
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1_000);
            thread::sleep(Duration::from_millis(ms));
        }
        Some("--write") => {
            let path = args.next().expect("path");
            let contents = args.next().unwrap_or_default();
            fs::write(path, contents).expect("write");
        }
        Some("--write-abs") => {
            let path = PathBuf::from(args.next().expect("path"));
            let contents = args.next().unwrap_or_default();
            fs::write(path, contents).expect("write-abs");
        }
        Some("--env-get") => {
            let key = args.next().expect("env name");
            match env::var(&key) {
                Ok(value) => {
                    print!("{value}");
                }
                Err(_) => std::process::exit(3),
            }
        }
        Some("--spawn-orphan-ms") => {
            let ms = args
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(30_000);
            let self_exe = env::current_exe().expect("current_exe");
            // Grandchild stays in the same process group (no setsid) so supervisor
            // process-group kill can reap it after cancel/timeout. Intentionally do
            // not wait here; parent also sleeps so both remain in the group.
            #[allow(clippy::zombie_processes)]
            let _child = Command::new(self_exe)
                .args(["--sleep-ms", &ms.to_string()])
                .spawn()
                .expect("spawn orphan");
            thread::sleep(Duration::from_millis(ms));
        }
        other => {
            eprintln!("unknown fake-worker mode: {other:?}");
            std::process::exit(2);
        }
    }
}
