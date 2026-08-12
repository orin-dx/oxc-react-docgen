//! Process-level tests for the `lsp` subcommand: drive the real binary's
//! stdin/stdout, not just its pure helper functions. `src/commands/lsp.rs`'s
//! own unit tests only assert `check_content_length`/`read_content_length`
//! in isolation — a regression at the `cmd_lsp` call site itself (e.g.
//! deleting the `check_content_length(len)?` call, or replacing a `return
//! Err` with `continue`) would leave those tests green while the actual
//! process-level behavior broke. SPEC-CLI-001c AC-001/002/004/005.

use std::io::Write;
use std::process::{Command, Output, Stdio};

// Mirrors `crates/cli/src/commands/lsp.rs`'s private `MAX_LSP_MESSAGE_BYTES`.
const MAX_LSP_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

fn run_lsp(args: &[&str], stdin_bytes: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oxc-react-docgen"))
        .arg("lsp")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn the real lsp binary");
    child.stdin.take().unwrap().write_all(stdin_bytes).expect("write to lsp stdin");
    child.wait_with_output().expect("wait for lsp process")
}

#[test]
fn oversized_content_length_exits_1_with_empty_stdout() {
    let frame = format!("Content-Length: {}\r\n\r\n", MAX_LSP_MESSAGE_BYTES + 1);
    let output = run_lsp(&[], frame.as_bytes());
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1, got {:?}. stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty(), "expected empty stdout, got {:?}", String::from_utf8_lossy(&output.stdout));
}

#[test]
fn headerless_frame_exits_1_with_empty_stdout() {
    let output = run_lsp(&[], b"X-Some-Other-Header: value\r\n\r\n");
    assert_eq!(output.status.code(), Some(1), "expected exit 1, got {:?}", output.status);
    assert!(output.stdout.is_empty(), "expected empty stdout, got {:?}", String::from_utf8_lossy(&output.stdout));
}

#[test]
fn well_formed_initialize_gets_a_response_frame_with_hover_false() {
    let body = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}});
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let mut input = format!("Content-Length: {}\r\n\r\n", body_bytes.len()).into_bytes();
    input.extend_from_slice(&body_bytes);

    // Follow with an `exit` notification so the process terminates cleanly.
    let exit_body = serde_json::json!({"jsonrpc": "2.0", "method": "exit"});
    let exit_bytes = serde_json::to_vec(&exit_body).unwrap();
    input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", exit_bytes.len()).as_bytes());
    input.extend_from_slice(&exit_bytes);

    let output = run_lsp(&[], &input);
    assert_eq!(output.status.code(), Some(0), "expected exit 0, stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("Content-Length:"), "expected a well-formed response frame, got {stdout:?}");
    assert!(stdout.contains("\"hoverProvider\":false"), "expected hoverProvider:false in the response, got {stdout:?}");
}

#[test]
fn verbose_mode_never_writes_tracing_output_to_stdout() {
    // A tracing::error! firing mid-session (here, via the oversized-Content-
    // Length path) must land on stderr only — LSP reserves stdout
    // exclusively for Content-Length-framed messages.
    let frame = format!("Content-Length: {}\r\n\r\n", MAX_LSP_MESSAGE_BYTES + 1);
    let output = run_lsp(&["-v"], frame.as_bytes());
    assert!(
        output.stdout.is_empty(),
        "stdout must contain no tracing output under -v, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!output.stderr.is_empty(), "expected the tracing::error! call to have written something to stderr");
}
