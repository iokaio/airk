# SPDX-License-Identifier: Apache-2.0
"""Read-only evidence collection; generates proposals, never changes policy."""
import hashlib
import json
from pathlib import Path
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent / "dependency-source-review"
CACHE = Path("C:/Users/tdotn/.cargo/registry")
REG = "index.crates.io-1949cf8c6b5b557f"

NOTES = {
"asn1-rs": "Owned certificate ASN.1 parsing; src/lib.rs forbids unsafe; BER parser default recursion 50. Parser correctness and arbitrary malformed input are not fully audited.",
"asn1-rs-derive": "Compiler procedural macro for ASN.1 derives; inspected token parsing/emission has no external I/O or process execution. Generated semantics are not formally proved.",
"asn1-rs-impl": "Compiler procedural macro encodes fixed OID/integer token literals; no external I/O/process found. Compile-time input handling is not a complete macro audit. Declares MIT OR Apache-2.0 but packages no standalone license text; retain this provenance qualification.",
"async-trait": "RMCP async interface procedural macro; token AST to boxed future expansion, no external I/O/process found; generated lifetime/Send behavior not fully audited.",
"atomic-waker": "Hyper wake coordination; atomic state guards unsafe interior-cell access and Send/Sync; callback panic/concurrency behavior not formally proved. Retain LICENSE-THIRD-PARTY.",
"base64": "PEM/HTTP encoding; default features include simd-unsafe. Inspected AVX2/NEON dispatch and unchecked encoding operations; no autonomous I/O/process. No all-architecture SIMD soundness claim.",
"bit-vec": "ASN.1 bit strings; bitmap indexing/allocation source inspected; no autonomous I/O/process found. Resource limits remain caller responsibility.",
"data-encoding": "Certificate data encoding; unsafe slice chunking/UTF8 views rely on checked bounds/alphabet contracts. No autonomous I/O/process found; no complete soundness proof.",
"der-parser": "Certificate DER/BER parser; forbids unsafe and default recursion 50. ber/tagged.rs explicitly resets a depth counter and ignores trailing bytes in one generic combinator; not a universal recursion/completeness guarantee. Native owned certificate files are separately bounded.",
"deranged": "Certificate time bounded integer helpers; unsafe unchecked constructors/range assumptions inspected; no autonomous I/O/process found. Range-contract correctness not fully proved.",
"getrandom": "Ring OS entropy; unsafe OS FFI, Linux /dev/urandom fallback; no downloader/subprocess found. Other target/custom/JS/RDRAND paths are not platform-qualified.",
"http": "Canonical HTTP types; unsafe header-map/byte representations. HeaderValue sensitivity is opt-in, not default credential redaction. Packaged VCS dirty flag retained.",
"http-body": "HTTP streaming trait; unsafe pin projections; size hints are not enforced budgets. Packaged VCS dirty flag retained.",
"http-body-util": "Bounded HTTP body adapters; Limited checks frame bytes, plain collect accumulates without its own cap. Unsafe pin projection inspected; callers must retain explicit limits.",
"httparse": "HTTP/1 parser with unsafe cursors/SIMD; build script runs configured RUSTC --version and emits cfgs, no download. No complete parser/request-smuggling/all-target soundness audit.",
"httpdate": "Hyper date formatting/parsing; forbids unsafe, no autonomous I/O/process found. Not a general calendar validation audit.",
"hyper": "Native HTTP/1 transport; caller-provided I/O/executor. Unsafe read buffers/upgrades/wakers; caller TLS/auth/budgets/deadlines retained. Optional HTTP2/C FFI not qualified.",
"hyper-util": "Tokio I/O adapter with unsafe ReadBuf projection. Optional legacy DNS/socket/proxy-environment paths exist; direct declaration selects tokio. Unified feature graph not executed.",
"lazy_static": "Parser shared initialization; Once/MaybeUninit unsafe publication inspected, no autonomous I/O/process found. User initializer behavior is outside this crate's guarantee.",
"num-bigint": "ASN.1 arbitrary precision integers; unsafe ASCII/UTF8 conversion and CPU arithmetic intrinsics inspected. Not constant-time crypto assurance; caller allocation limits remain required.",
"num-conv": "Time integer conversions; pure arithmetic/traits, no external I/O/process or unsafe found in inspected source. No exhaustive arithmetic proof.",
"oid-registry": "Certificate OID names; build script reads packaged assets/oid_db.txt and writes OUT_DIR/oid_db.rs. No subprocess/download; fixed verified archive DB is a compiler input. Runtime forbids unsafe.",
"pem": "Owned CA PEM armor; pure parse/encode, no unsafe/external I/O/process found in inspected source. Decoding is not certificate trust validation.",
"powerfmt": "Time formatting; unsafe MaybeUninit buffers rely on initialized width/precision contracts. No autonomous I/O/process found; full soundness not proved.",
"rcgen": "Owner-private CA/leaf issuance using ring SystemRandom; forbids unsafe, no build script. Issuer CA parser assumes rather than validates CA. Private key DER clones are not automatically zeroized; optional zeroize omitted by AIRK.",
"ring": "TLS cryptography and OS entropy via Rust/C/ASM FFI and CPU dispatch. Build script executes compiler/assembler, can consume shipped pregenerated objects, optional regeneration Perl/NASM; no download found. Packaged dirty VCS metadata and nested Apache/ISC notices retained; no crypto/prebuilt reproducibility audit.",
"rusticata-macros": "ASN.1 parser combinator macros; inspected source has no unsafe/autonomous I/O/process. Composed parser completeness/resource bounds remain caller responsibilities.",
"rustls": "TLS engine, forbids unsafe, ring/WebPKI delegation; build script probes optional nightly cfg. Explicit KeyLogFile can read SSLKEYLOGFILE/write secrets; AIRK must not install logger. No protocol/crypto correctness certification.",
"rustls-pki-types": "TLS key/certificate/name containers; explicit caller PEM file APIs and fixed-layout unsafe IPv6 conversion. Zeroization is API-specific, not all copies. No independent trust validation in types.",
"rustls-webpki": "TLS chain and identity validation with explicit work budgets; no unsafe/autonomous I/O/process found in inspected source. Chain and name validation are separate caller obligations; no complete cryptographic/parser audit.",
"sse-stream": "RMCP SSE parser; unfinished lines/event data grow without intrinsic byte bound; optional tracing logs invalid content. Exception conditional on caller byte/event/deadline limits and logging policy.",
"subtle": "Constant-time selection helpers use unsafe volatile optimization barrier; no external I/O/process. Best-effort compiler barrier, not universal constant-time proof.",
"synstructure": "ASN.1 derive token AST helper; no external I/O/process or unsafe found in inspected source. Generated-code behavior not fully audited.",
"time": "CA certificate validity intervals; private unsafe validated date/range operations. Optional local-offset has OS FFI; declared default std/macros are not full resolved feature proof. No autonomous process/download found.",
"time-core": "Shared time/calendar constants and arithmetic; no unsafe/external I/O/process found in inspected source. No exhaustive calendar arithmetic proof.",
"time-macros": "Compiler-executed date/time literal macros; unsafe UTF8/slice and emitted unchecked validated constructors. No external I/O/process found; no full generated-code soundness proof.",
"tokio-rustls": "Async TLS over caller-provided I/O, no autonomous dial/resolve; no unsafe/build script found. Caller owns deadlines, authorization, CA validation and cancellation.",
"tower-service": "Readiness/call trait, forbids unsafe; implementers must release admission reservations on cancellation. No independent scheduler/admission guarantee.",
"try-lock": "CONDITIONAL: deprecated safe try_lock_order is documented unsound for weak ordering. Inspected want callers use checked try_lock_explicit SeqCst only; no deprecated callers found in AIRK/Hyper/want. Do not certify crate-wide soundness; reject future unchecked/weak-order usage.",
"untrusted": "Checked bounded input reader/read_all rejects trailing input; no unsafe/build script/autonomous I/O/process found. Does not validate higher-level certificate semantics.",
"want": "Hyper readiness uses checked try_lock_explicit SeqCst at both inspected calls, atomic state and caller wakers; no unsafe/external I/O found. Conditional on try-lock checked usage; no concurrency proof.",
"windows-sys": "Getrandom Windows OS bindings: generated unsafe extern declarations/macros; platform ABI/handle lifetime is caller responsibility. No build script. No cross-target execution qualification.",
"windows-targets": "Architecture-specific Windows linking macros select OS import libraries/raw-dylib. No downloader/process/build script. Imported ABI correctness and other targets not runtime-qualified.",
"x509-parser": "Owned CA metadata parsing; forbids unsafe, explicit Read/PEM APIs; parsing is not trust/chain validation. Rustls WebPKI validates peer certificates separately. No complete parser audit.",
"yasna": "RCGen ASN.1 DER writer/reader; forbids unsafe, no autonomous I/O/process/build script found. Encoding correctness not exhaustively proved.",
"zeroize": "Volatile writes/compiler barriers and Drop wrapper; unsafe slice operations inspected. Copies/reallocation/explicit unwrapped buffers remain outside blanket erasure guarantee.",
}

def sha(b):
    return hashlib.sha256(b).hexdigest()

def main():
    lock_bytes = (ROOT / "Cargo.lock").read_bytes()
    policy_bytes = (ROOT / "supply-chain/source-policy.json").read_bytes()
    lock = tomllib.loads(lock_bytes.decode())
    old = {tuple(p.get(k) for k in ("name","version","source","checksum")) for p in json.loads(policy_bytes)["packages"]}
    packages = [p for p in lock["package"] if p.get("source") and tuple(p.get(k) for k in ("name","version","source","checksum")) not in old]
    assert len(packages) == 54, len(packages)
    rows = []
    for p in packages:
        name, version = p["name"], p["version"]
        archive = CACHE / "cache" / REG / f"{name}-{version}.crate"
        src = CACHE / "src" / REG / f"{name}-{version}"
        raw = archive.read_bytes()
        assert sha(raw) == p["checksum"], name
        manifest = tomllib.loads((src / "Cargo.toml").read_text(encoding="utf-8"))
        files = []
        with tarfile.open(archive, "r:gz") as tf:
            for m in tf:
                if m.isdir():
                    continue
                assert m.isfile(), (name, m.name, m.type)
                prefix = f"{name}-{version}/"
                assert m.name.startswith(prefix), m.name
                path = m.name[len(prefix):]
                assert path and ".." not in Path(path).parts and not Path(path).is_absolute()
                data = tf.extractfile(m).read()
                assert (src / path).read_bytes() == data, (name, path)
                files.append({"path":path,"bytes":len(data),"sha256":sha(data)})
        files.sort(key=lambda x:x["path"])
        license_files = [f for f in files if any(t in Path(f["path"]).name.lower() for t in ("license", "copying", "notice", "copyright"))]
        build = manifest["package"].get("build")
        if build is None and (src / "build.rs").is_file():
            build = "build.rs"
        vcs = json.loads((src/".cargo_vcs_info.json").read_text()) if (src/".cargo_vcs_info.json").is_file() else None
        note = NOTES.get(name)
        if name.startswith("windows_"):
            note = "Windows architecture import-library package; build.rs only emits CARGO_MANIFEST_DIR/lib link search. Published binary import libraries are exact registry-checksummed bytes, not independently regenerated or qualified on this architecture. No network/subprocess found."
        assert note, name
        rows.append({"identity":{k:p[k] for k in ("name","version","source","checksum")}, "archive_bytes":len(raw),"packaged_files":len(files),"packaged_tree_sha256":sha(json.dumps(files,sort_keys=True,separators=(",",":")).encode()),"cache_matches_all_packaged_files":True,"archive_links":0,"manifest_sha256":sha((src/"Cargo.toml").read_bytes()),"original_manifest_sha256":sha((src/"Cargo.toml.orig").read_bytes()) if (src/"Cargo.toml.orig").exists() else None,"declared_license":manifest["package"].get("license"),"license_files":license_files,"declared_repository":manifest["package"].get("repository"),"packaged_vcs_metadata_unverified":vcs,"effective_build_script":build,"procedural_macro":manifest.get("lib",{}).get("proc-macro",False),"exception_rationale_and_limits":note})
    OUT.mkdir(parents=True,exist_ok=True)
    report={"schema":"airk.dependency-source-review-proposal/1","status":"PROPOSED_NOT_APPROVED","date":"2026-11-07","lock_sha256":sha(lock_bytes),"source_policy_before_sha256":sha(policy_bytes),"scope":"Exact registry archive identity, extracted-cache equality, declared licenses/build behavior and targeted source inspection. Not full safe-to-deploy audit, upstream publisher authentication, advisory clearance, platform execution or legal approval.","packages":rows}
    (OUT/"review.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    (OUT/"source-policy-additions.json").write_text(json.dumps([r["identity"] for r in rows],indent=2)+"\n",encoding="utf-8")
    lines=[]
    for r in rows:
        p=r["identity"]
        note="PROPOSED exact-version exception for native HTTPS/TLS transport; source review 2026-11-07. "+r["exception_rationale_and_limits"]+" Not a full security audit or current advisory clearance."
        lines.extend([f'[[exemptions.{p["name"]}]]',f'version = {json.dumps(p["version"])}','criteria = "safe-to-deploy"',f'notes = {json.dumps(note)}',""])
    (OUT/"exemptions-proposal.toml").write_text("\n".join(lines),encoding="utf-8")
    tomllib.loads((OUT/"exemptions-proposal.toml").read_text(encoding="utf-8"))
    print(json.dumps({"packages":len(rows),"packaged_files":sum(r["packaged_files"] for r in rows),"archive_bytes":sum(r["archive_bytes"] for r in rows),"report_sha256":sha((OUT/"review.json").read_bytes()),"output":str(OUT)}))

if __name__ == "__main__":
    main()
