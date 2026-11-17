# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

base = Path(__file__).parent
source = base / 'foundation-source'
changes = {}
name = 'crates/airk-store/src/current/journal.rs'
old = (source / name).read_text(encoding='utf-8')
changes[name] = (old, old + (base / 'restart-metadata.rs').read_text(encoding='utf-8'))
name = 'crates/airk-store/src/service_collection_tests.rs'
old = (source / name).read_text(encoding='utf-8')
new = old.replace('    assert_eq!(recovered.token()?, pending.target.token()?);', '''    assert_eq!(recovered.token()?, pending.target.token()?);
    assert_eq!(recovered.settled_native_head()?, Some(recovered.head().clone()));''')
needle = '    assert_eq!(current.phase()?, TransactionPhase::Applying);'
assert old.count(needle) == 1
# The new reader is opened only after dropping the active writer below; a
# pre-Applying retained read is expressly NOT a current-state authority.
start = new.index(needle)
at = new.index('    drop(current);', start) + len('    drop(current);')
new = new[:at] + '''
    {
        let applying_read = store.current_read(&scope)?;
        assert!(applying_read.settled_native_head()?.is_none(),
            "authenticated Applying base must not be relabelled settled");
    }''' + new[at:]
changes[name] = (old, new)
patch = []
for name, (old, new) in changes.items():
    dest = base / 'restart-metadata-source' / name
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(new, encoding='utf-8', newline='\n')
    patch.extend(difflib.unified_diff(old.splitlines(True), new.splitlines(True),
                 fromfile='a/' + name, tofile='b/' + name))
(base / 'restart-metadata-after-foundation.patch').write_text(''.join(patch), encoding='utf-8', newline='\n')
