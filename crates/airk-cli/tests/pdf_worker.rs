// SPDX-License-Identifier: Apache-2.0
//! Exercise PDF ingestion through the distributed executable, including worker dispatch.
use serde_json::Value;
use std::{fs, process::Command};

fn pdf() -> Vec<u8> {
    let stream = "BT /F1 12 Tf 10 100 Td (Isolated PDF evidence.) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
    ];
    let mut bytes = "%PDF-1.4\n".to_owned();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = bytes.len();
    bytes.push_str("xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        bytes.push_str(&format!("{offset:010} 00000 n \n"));
    }
    bytes.push_str(&format!(
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    ));
    bytes.into_bytes()
}

#[test]
fn distributed_cli_indexes_pdf_in_its_worker_and_preserves_source_locations() -> anyhow::Result<()>
{
    let source = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let original = pdf();
    fs::write(source.path().join("original.pdf"), &original)?;
    let command = |args: &[&str]| -> anyhow::Result<Value> {
        let output = Command::new(env!("CARGO_BIN_EXE_airk"))
            .arg("--root")
            .arg(source.path())
            .arg("--data-dir")
            .arg(state.path())
            .args(["--collection", "isolated-pdf"])
            .args(args)
            .env("AIRK_USAGE_MODE", "disabled")
            .output()?;
        assert!(
            output.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    command(&["collections", "vectors", "set", "--enabled", "false"])?;
    command(&["index"])?;
    let envelope = command(&[
        "tools",
        "call",
        "airk_read_file",
        "--args",
        r#"{"path":"original.pdf","start_line":1,"end_line":3}"#,
    ])?;
    assert_ne!(envelope["isError"], true, "{envelope}");
    let evidence = envelope["content"][0]["text"].as_str().unwrap();
    assert!(evidence.contains("Isolated PDF evidence"), "{evidence}");
    assert!(evidence.contains("Page"), "{evidence}");
    assert_eq!(fs::read(source.path().join("original.pdf"))?, original);
    // Reindex exercises persisted extraction-profile reuse through the same CLI.
    command(&["index"])?;
    Ok(())
}
