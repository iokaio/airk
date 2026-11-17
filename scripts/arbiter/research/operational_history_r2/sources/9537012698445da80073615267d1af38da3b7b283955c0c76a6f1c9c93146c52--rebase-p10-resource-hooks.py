# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
import hashlib
import json

base = Path(__file__).resolve().parent
live = base / 'p10-worktree/crates/airk-cli/tests/combined_reads/split.rs'
before = live.read_text(encoding='utf-8')
after = before

def replace(old, new, count=1):
    global after
    assert after.count(old) == count, (old[:100], after.count(old), count)
    after = after.replace(old, new)

replace('#[cfg(feature = "embedding-worker")]\nfn scope_refusal', '#[path = "split_metrics.rs"]\nmod metrics;\n\n#[cfg(feature = "embedding-worker")]\nfn scope_refusal')
replace('async fn terminal(f: &Fixture, operation: Incarnation, canceled: bool) -> Result<SplitSnapshot> {', '''async fn terminal(
    f: &Fixture,
    operation: Incarnation,
    canceled: bool,
    capture: &mut Option<metrics::Capture>,
    service: &mut OwnedProcess,
) -> Result<SplitSnapshot> {''')
# Match only the terminal loop; leave the cancellation reservation loop intact.
replace('''    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let snapshot = state(f, operation).await?;''', '''    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let snapshot = state(f, operation).await?;
        if let Some(observer) = capture.as_mut() {
            observer.observe(f, service, &snapshot).await?;
        }''')
replace('    let _service = f.start().await?;', '    let mut _service = f.start().await?;', 2)
# The setup grant changes catalog revision. Obtain/warm the old evidence scopes
# only after that grant exists, preserving a successful pre-split proof.
replace('    let old = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;', '''    let mut capture = metrics::Capture::start(&mut f, &mut _service, "complete").await?;
    let old = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;''')
replace('    let reviewed = draft(&f, 131).await?;', '''    let reviewed = draft(&f, 131).await?;
    if let Some(observer) = capture.as_mut() {
        observer.destination(&f, &reviewed)?;
    }''')
replace('    let complete = terminal(&f, operation, false).await?;', '    let complete = terminal(&f, operation, false, &mut capture, &mut _service).await?;')
replace('''    drop(mcp);
    f.assert_sources_unchanged()?;
    Ok(())''', '''    drop(mcp);
    f.assert_sources_unchanged()?;
    if let Some(observer) = capture.take() {
        observer.finish(&f, &mut _service, &complete).await?;
    }
    Ok(())''')
replace('''    let f = Fixture::new(true)?;
    let mut _service = f.start().await?;
    let reviewed = draft(&f, 141).await?;''', '''    let mut f = Fixture::new(true)?;
    let mut _service = f.start().await?;
    let mut capture = metrics::Capture::start(&mut f, &mut _service, "cancel").await?;
    let reviewed = draft(&f, 141).await?;
    if let Some(observer) = capture.as_mut() {
        observer.destination(&f, &reviewed)?;
    }''')
replace('''    let snapshot = loop {
        let snapshot = state(&f, operation).await?;''', '''    let snapshot = loop {
        let snapshot = state(&f, operation).await?;
        if let Some(observer) = capture.as_mut() {
            observer.observe(&f, &mut _service, &snapshot).await?;
        }''')
replace('    terminal(&f, operation, true).await?;', '    let completed_cancel = terminal(&f, operation, true, &mut capture, &mut _service).await?;')
replace('''    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())''', '''    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    if let Some(observer) = capture.take() {
        observer.finish(&f, &mut _service, &completed_cancel).await?;
    }
    Ok(())''')
(base / 'p10-split-with-metrics.proposed.rs').write_text(after, encoding='utf-8', newline='\n')
handoff_path = base / 'p10-resource-hooks-handoff.json'
handoff = json.loads(handoff_path.read_text(encoding='utf-8'))
handoff['base_sha256'] = hashlib.sha256(live.read_bytes()).hexdigest()
handoff['plan'] = 'artifacts/v190/p10-resource-campaign-plan-r3.json'
handoff['rebase'] = 'Preserves live stalled-native diagnostics, paused-original quiescence proof and exact held-owner Accepted reservation cancellation boundary. Native metric grant/context setup precedes successful old scope warming.'
handoff['query'] = 'Four persistent actual MCP clients; separately native-authenticated virtual context and explicit Project0/index0 grant. Baseline2/during<=30/settled2 waves; single attempt max_bytes16384, complete typed rejections retained, citations/hashes checked. Complete scenario requires4 successes while exact child Running at both wave boundaries; metadata-only cancel requires zero child jobs and labels contention not applicable. Both require4 baseline and4 settled successes.'
handoff_path.write_text(json.dumps(handoff, indent=2) + '\n', encoding='utf-8')
print(json.dumps({'base_sha256':handoff['base_sha256'], 'live_bytes':live.stat().st_size, 'clone_bytes':len(after.encode())}))
