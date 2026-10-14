// SPDX-License-Identifier: Apache-2.0
//! Original deterministic publication fixture, not a real provider qualification.
use std::io::{self, BufRead, Read, Write};

fn lsp() -> io::Result<()> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    for _ in 0..32 {
        let mut length = None;
        for _ in 0..8 {
            let mut header = String::new();
            if input.read_line(&mut header)? == 0 {
                return Ok(());
            }
            assert!(header.len() <= 1024);
            if header == "\r\n" {
                break;
            }
            if let Some(value) = header.strip_prefix("Content-Length:") {
                length = Some(value.trim().parse::<usize>().unwrap());
            }
        }
        let length = length.expect("missing frame length");
        assert!(length <= 65536);
        let mut bytes = vec![0; length];
        input.read_exact(&mut bytes)?;
        let body = String::from_utf8(bytes).unwrap();
        // AIRK emits compact JSON. This original fixture accepts only its known
        // requests; it is deliberately not a reusable JSON or LSP implementation.
        let Some(tail) = body.split_once("\"id\":").map(|(_, tail)| tail) else {
            if body.contains("\"method\":\"exit\"") {
                return Ok(());
            }
            continue;
        };
        let id: String = tail.chars().take_while(char::is_ascii_digit).collect();
        assert!(!id.is_empty());
        let result = if body.contains("\"method\":\"initialize\"") {
            r#"{"capabilities":{"documentSymbolProvider":true,"positionEncoding":"utf-8"},"serverInfo":{"name":"original-publication-fixture","version":"1"}}"#
        } else if body.contains("\"method\":\"textDocument/documentSymbol\"") {
            r#"[{"name":"one","kind":12,"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":24}},"selectionRange":{"start":{"line":0,"character":7},"end":{"line":0,"character":10}}}]"#
        } else if body.contains("\"method\":\"shutdown\"") {
            "null"
        } else {
            panic!("unexpected fixture request: {body}");
        };
        let response = format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{result}}}"#);
        write!(
            output,
            "Content-Length: {}\r\n\r\n{}",
            response.len(),
            response
        )?;
        output.flush()?;
    }
    panic!("fixture frame bound exceeded");
}

fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("lsp") => lsp(),
        Some("--options=NONE") => {
            println!(
                r#"{{"_type":"tag","name":"one","path":"src/lib.rs","line":1,"end":1,"kind":"function","signature":"() -> u32"}}"#
            );
            Ok(())
        }
        _ if args.get(1).is_some_and(|arg| arg == "stdout") => {
            println!("REQ-42 must preserve this original page.");
            Ok(())
        }
        _ => panic!("unexpected original fixture invocation"),
    }
}
