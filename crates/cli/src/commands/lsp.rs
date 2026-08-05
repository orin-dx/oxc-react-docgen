//! LSP (Language Server Protocol) command scaffold for oxc-react-docgen.
//!
//! Provides IDE hover and component prop documentation in editors (VS Code, Neovim, Helix).

use std::io::{self, BufRead, Read, Write};

use miette::Result;

pub fn cmd_lsp() -> Result<()> {
    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    loop {
        let mut content_length: Option<usize> = None;
        let mut line = String::new();

        loop {
            line.clear();
            if stdin_lock.read_line(&mut line).unwrap_or(0) == 0 {
                return Ok(()); // EOF
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some(header) = line.strip_prefix("Content-Length: ") {
                content_length = header.trim().parse().ok();
            }
        }

        let Some(len) = content_length else {
            continue;
        };

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
                            "result": {
                                "capabilities": {
                                    "textDocumentSync": 1,
                                    "hoverProvider": true
                                },
                                "serverInfo": {
                                    "name": "oxc-react-docgen-lsp",
                                    "version": env!("CARGO_PKG_VERSION")
                                }
                            }
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
