//! LSP (Language Server Protocol) command scaffold for oxc-react-docgen.
//!
//! Provides IDE hover and component prop documentation in editors (VS Code, Neovim, Helix).

use std::io::{self, BufRead, Read, Write};

use miette::Result;

/// Cap on a single LSP frame's declared `Content-Length`, checked *before*
/// `vec![0u8; len]` allocates — `Content-Length` is client-controlled input
/// with no upper bound of its own.
const MAX_LSP_MESSAGE_BYTES: usize = 16 * 1024 * 1024; // 16 MiB

/// Reject an oversized frame before its receive buffer is allocated.
fn check_content_length(len: usize) -> Result<()> {
    if len > MAX_LSP_MESSAGE_BYTES {
        tracing::error!(
            "LSP frame declared Content-Length {len} bytes, exceeding the {MAX_LSP_MESSAGE_BYTES}-byte cap; closing connection"
        );
        return Err(miette::miette!(
            "LSP frame declared Content-Length {len} bytes, exceeding the {MAX_LSP_MESSAGE_BYTES}-byte cap"
        ));
    }
    Ok(())
}

/// Reads header lines up to the blank-line terminator and returns the parsed
/// `Content-Length`, if any.
///
/// `Err(())` means the stream ended before any header line was read at all —
/// a normal, expected way for a client to close the connection.
/// `Ok(None)` means a header block *was* read (the loop reached the blank
/// line) but it contained no usable `Content-Length` — missing header or an
/// unparsable value. That body's byte length is unknowable, so the caller
/// must not keep reading from the stream as if nothing happened: doing so
/// desyncs every subsequent frame, since the unconsumed body bytes get read
/// as the start of the *next* header block.
fn read_content_length<R: BufRead>(reader: &mut R) -> Result<Option<usize>, ()> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return Err(());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(header) = line.strip_prefix("Content-Length: ") {
            content_length = header.trim().parse().ok();
        }
    }

    Ok(content_length)
}

/// The `initialize` response's `result` payload — factored out so capability
/// advertisement is testable without driving the stdin/stdout loop.
fn initialize_result() -> serde_json::Value {
    serde_json::json!({
        "capabilities": {
            "textDocumentSync": 1,
            // No textDocument/hover handler exists in the method dispatch
            // below (it falls into the `_ => {}` catch-all) — advertising
            // `true` would make a client's hover request, which carries an
            // `id`, hang forever waiting for a response that never comes.
            "hoverProvider": false
        },
        "serverInfo": {
            "name": "oxc-react-docgen-lsp",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

pub fn cmd_lsp() -> Result<()> {
    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    loop {
        let content_length = match read_content_length(&mut stdin_lock) {
            Err(()) => return Ok(()), // EOF
            Ok(v) => v,
        };

        let Some(len) = content_length else {
            tracing::error!(
                "LSP frame's header block had no usable Content-Length; closing connection to avoid a desynced stream"
            );
            return Err(miette::miette!("received an LSP frame without a usable Content-Length header"));
        };

        check_content_length(len)?;

        let mut buf = vec![0u8; len];
        if stdin_lock.read_exact(&mut buf).is_err() {
            return Ok(());
        }

        let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&buf) else {
            continue;
        };

        if let Some(method) = msg["method"].as_str() {
            let id = msg.get("id");
            match method {
                "initialize" => {
                    if let Some(id) = id {
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": initialize_result()
                        });
                        send_response(&mut stdout_lock, &response);
                    }
                }
                "shutdown" => {
                    if let Some(id) = id {
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": null
                        });
                        send_response(&mut stdout_lock, &response);
                    }
                }
                "exit" => break,
                _ => {}
            }
        }
    }

    Ok(())
}

fn send_response<W: Write>(out: &mut W, response: &serde_json::Value) {
    if let Ok(json) = serde_json::to_string(response) {
        let header = format!("Content-Length: {}\r\n\r\n", json.len());
        let _ = out.write_all(header.as_bytes());
        let _ = out.write_all(json.as_bytes());
        let _ = out.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oversized_content_length_is_rejected() {
        let result = check_content_length(MAX_LSP_MESSAGE_BYTES + 1);
        assert!(result.is_err(), "expected a Content-Length past the cap to be rejected, got {:?}", result);
    }

    #[test]
    fn test_content_length_at_cap_is_accepted() {
        let result = check_content_length(MAX_LSP_MESSAGE_BYTES);
        assert!(result.is_ok(), "expected a Content-Length exactly at the cap to be accepted, got {:?}", result);
    }

    #[test]
    fn test_hover_not_advertised_without_a_handler() {
        let result = initialize_result();
        assert_eq!(
            result["capabilities"]["hoverProvider"],
            serde_json::json!(false),
            "hoverProvider must stay false until a textDocument/hover handler exists — a client hover \
             request carries an id and nothing replies to it today"
        );
    }

    #[test]
    fn test_headerless_frame_is_reported_as_malformed_not_eof() {
        // Blank line reached with no Content-Length header at all — the
        // pre-fix code left `content_length: None` and `continue`d straight
        // past the body that frame's sender still wrote, permanently
        // desyncing every header block read afterward.
        let mut reader = std::io::Cursor::new(b"X-Some-Other-Header: value\r\n\r\n".to_vec());
        assert_eq!(read_content_length(&mut reader), Ok(None));
    }

    #[test]
    fn test_unparsable_content_length_is_reported_as_malformed() {
        let mut reader = std::io::Cursor::new(b"Content-Length: not-a-number\r\n\r\n".to_vec());
        assert_eq!(read_content_length(&mut reader), Ok(None));
    }

    #[test]
    fn test_true_stream_eof_is_distinct_from_a_malformed_header_block() {
        // No bytes at all (client closed the connection) is a normal,
        // expected way to end the loop — must not be conflated with a
        // header block that was read but had no usable Content-Length.
        let mut reader = std::io::Cursor::new(Vec::new());
        assert_eq!(read_content_length(&mut reader), Err(()));
    }
}
