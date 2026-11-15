//! The `lin` binary writing into a pipe whose reader has gone (`lin run ... | head`)
//! stops quietly, like other command-line tools, instead of panicking with
//! `failed printing to stdout` and exit code 101.

#![cfg(unix)]

use std::io::Read;
use std::process::{Command, Stdio};

#[test]
fn closed_stdout_pipe_does_not_panic() {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let base = std::env::temp_dir().join(format!("lin-cli-pipe-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let data = base.join("data");
    let program = base.join("load.lin");
    let rows = (0..20_000).map(|i| format!(r#"{{ id: "r{i}", v: {i} }}"#)).collect::<Vec<_>>().join(",");
    std::fs::write(&program, format!("col t {{ id: text, v: i64 }}\ninsert t [{rows}]")).unwrap();
    let bin = env!("CARGO_BIN_EXE_lin");
    let load = Command::new(bin).arg("--data").arg(&data).arg("run").arg("--file").arg(&program).output().unwrap();
    assert!(load.status.success(), "load failed: {}", String::from_utf8_lossy(&load.stderr));

    let mut child = Command::new(bin)
        .arg("--data").arg(&data).arg("run").arg("t | take all")
        .stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    let mut first = [0u8; 16];
    child.stdout.as_mut().unwrap().read_exact(&mut first).unwrap();
    drop(child.stdout.take());
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    let status = child.wait().unwrap();
    assert!(!err.contains("panicked"), "panicked on a closed pipe: {err}");
    assert_ne!(status.code(), Some(101), "exit code 101 (panic) on a closed pipe");
}

/// `lin cache cli` with commands piped in prints only the replies: no banner and no
/// prompts mixed into the output a script reads.
#[test]
fn piped_cache_cli_prints_no_prompts() {
    use std::io::Write;
    let bin = env!("CARGO_BIN_EXE_lin");
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let addr = format!("127.0.0.1:{port}");
    let mut server = Command::new(bin).args(["cache", "serve", "--listen", &addr]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let start = std::time::Instant::now();
    while std::net::TcpStream::connect(&addr).is_err() {
        assert!(start.elapsed() < std::time::Duration::from_secs(20), "cache server did not start");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let mut cli = Command::new(bin).args(["cache", "cli", "--connect", &addr]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    cli.stdin.take().unwrap().write_all(b"SET greeting hello\nGET greeting\nQUIT\n").unwrap();
    let out = cli.wait_with_output().unwrap();
    let _ = server.kill();
    let _ = server.wait();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "cli failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("hello"), "reply missing: {stdout:?}");
    assert!(!stdout.contains("cacha ("), "prompt in piped output: {stdout:?}");
    assert!(!stdout.contains("Connected to"), "banner in piped output: {stdout:?}");
}
