# SPDX-License-Identifier: Apache-2.0
"""Repeat the reviewed native Admin canonical-handler extraction.

Run on the pre-extraction source or rerun after it (a checked no-op). Native
bridge routing/admission is a separate behavior step, intentionally not applied
by this mechanical transformer. The original match's normalized digest was
recorded before that step added its own transport arm.
"""
from __future__ import annotations

import hashlib
from pathlib import Path

NORMALIZED_MATCH_SHA256 = "7eab8faaee16c36aaecdbde96d5b0f9e10eee8c6e4efe39e2217e997271910db"


def main() -> None:
    path = Path(__file__).resolve().parents[2] / "crates/airk-service/src/service_runtime.rs"
    source = path.read_text(encoding="utf-8")
    if "fn canonical_control(" in source:
        assert source.count("fn canonical_control(") == 1
        assert "match request.method {" not in source
        print("Canonical control already extracted; no bytes changed.")
        return
    start = source.index("        match request.method {", source.index("async fn connection("))
    end = source.index("\n        }\n    };\n    if let ControlResponse::Failure", start) + len("\n        }")
    original = source[start:end]
    moved = original.replace("match request.method {", "match method {", 1)
    assert moved.replace("match method {", "match request.method {", 1) == original
    digest = hashlib.sha256(moved.encode()).hexdigest()
    if digest != NORMALIZED_MATCH_SHA256:
        raise SystemExit(f"Unreviewed match identity {digest}; source left unchanged.")
    replacement = """        let (response, requested_shutdown)=canonical_control(state.clone(),request.method).await?;
        shutdown|=requested_shutdown;
        response"""
    helper = """
/// Canonical control behavior shared by the authenticated pipe and native Admin.
/// Boxed at this boundary so the private facade cannot form an async type cycle.
fn canonical_control(
    state: Arc<State>,
    method: ControlMethod,
) -> std::pin::Pin<Box<dyn std::future::Future<Output=Result<(ControlResponse,bool)>> + Send>> {
    Box::pin(async move {
        let mut shutdown=matches!(method,ControlMethod::Shutdown);
        let response=
""" + moved + """;
        Ok((response,shutdown))
    })
}
"""
    source = source[:start] + replacement + source[end:]
    marker = "/// Accepted shutdown belongs to the service, independently of reply delivery."
    assert source.count(marker) == 1
    path.write_text(source.replace(marker, helper + "\n" + marker, 1), encoding="utf-8", newline="\n")
    print(f"Moved {len(moved.splitlines())} exact normalized lines: {digest}")


if __name__ == "__main__":
    main()
