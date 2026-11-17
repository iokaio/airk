# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib
p=Path(__file__).resolve().parent;r=p.parents[2]
names=['crates/airk-store/src/current/recent.rs','crates/airk-store/src/current/recent_tests.rs']
base={}
for n in names:
 data=(r/n).read_bytes();base[n]=hashlib.sha256(data).hexdigest()
 for layer in ['base','source']:
  path=p/layer/n;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)
(p/'BASE.json').write_text(json.dumps({'schema':1,'kind':'exact live raw preimages; no F10/F11 mirror overlay','files':base},indent=2)+'\n',encoding='utf-8')
f=p/'source'/names[0];s=f.read_text(encoding='utf-8');start=s.index('impl CurrentRead {\n    pub fn recent_changes(');end=s.index('\n/// All keys/values',start)
s=s[:start]+'''impl CurrentRead {
    pub fn recent_changes(&self, now_unix_seconds: u64) -> Result<RecentChanges> {
        // Ordinary queries/diagnostics retain full unexpired-group verification.
        // Selection is private preparation, not a caller-granted read authority.
        read_recent(
            &self.head().scope,
            self.token()?,
            now_unix_seconds,
            |_| Some(0),
            &mut |key| self.map_get(map::MapDomain::RecentChanges, key),
        )
    }
}

/// Internal preparation engine. The public reader ALWAYS selects all groups.
/// A future qualified native owner must bind the exact enabled recent-component
/// policy/maintenance evidence before exposing selected materialization. This
/// crate-internal closure is not a public bypass or a serialized capability.
pub(super) fn read_recent(
    scope: &CurrentScope,
    token: CurrentToken,
    now_unix_seconds: u64,
    select_after: impl FnOnce(&RecentChanges) -> Option<u64>,
    get: &mut impl FnMut(&[u8]) -> Result<Option<Vec<u8>>>,
) -> Result<RecentChanges> {
    let Some(bytes) = get(HEADER)? else {
        return Ok(RecentChanges {
            coverage: coverage(token.sequence, CoverageReason::Unsupported),
            parents: vec![], events: vec![],
            view_digest: framed_digest(b"recent-change-view/1", &[b"absent"]),
        });
    };
    let mut value = manifest(&bytes, scope, token.sequence)?;
    if value.generation != token.sequence {
        return Ok(RecentChanges {
            coverage: coverage(token.sequence, CoverageReason::Omitted),
            parents: vec![], events: vec![],
            view_digest: framed_digest(b"recent-change-view/1", &[&bytes, b"publication-gap"]),
        });
    }
    let expired_through = value.groups.iter()
        .filter(|g| now_unix_seconds.saturating_sub(g.observed_unix_seconds) >= AGE_SECONDS)
        .map(|g| g.generation).max();
    let mut parents = Vec::new();
    let mut first = token.sequence;
    for group in &value.groups {
        if expired_through.is_some_and(|last| group.generation <= last) {
            expire(&mut value.coverage, group.generation, CoverageReason::Expired);
            continue;
        }
        if parents.is_empty() { first = group.parent.generation; }
        parents.push(group.parent);
    }
    let view_digest = framed_digest(b"recent-change-view/1", &[
        &bytes, &first.to_be_bytes(), &serde_json::to_vec(&value.coverage)?,
    ]);
    let mut view = RecentChanges { coverage: value.coverage, parents,
        events: vec![], view_digest };
    // Header identity, full independent coverage floors and age are established
    // BEFORE deciding whether a verified baseline needs event materialization.
    // Never compute a selected-only view digest or cache the wall-clock result.
    if let Some(after) = select_after(&view) {
        for group in &value.groups {
            if !expired_through.is_some_and(|last| group.generation <= last)
                && group.generation > after {
                view.events.extend(group_events(group, get)?);
            }
        }
    }
    Ok(view)
}
''' +s[end:];f.write_text(s,encoding='utf-8',newline='\n')
