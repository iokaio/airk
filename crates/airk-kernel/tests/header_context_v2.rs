// SPDX-License-Identifier: Apache-2.0
//! Captured local compiler arguments select headers without build execution.
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::Result;
use std::fs;

#[test]
fn captured_header_configuration_invalidation_is_scoped_and_override_wins() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("build/include"))?;
    let source = "class Beacon { public: int read() const; };\n";
    fs::write(root.path().join("build/include/beacon.h"), source)?;
    fs::write(root.path().join("unrelated.h"), "int unrelated(void);\n")?;
    let configuration = root.path().join("build/compile_commands.json");
    let cpp=br#"[{"directory":".","file":"include/beacon.h","arguments":["unexecuted-compiler","-x","c++","include/beacon.h"]}]"#;
    fs::write(&configuration, cpp)?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    let policy = serde_json::from_value(
        serde_json::json!({"version":2,"rules":[{"selector":{"kind":"header_context","prefix":"build/include","configuration":"build/compile_commands.json"},"case_sensitive":true,"adapter_id":"cpp"}]}),
    )?;
    kernel.configure_extraction_policy(policy)?;
    kernel.index()?;
    let store = FileStore::new(state.path().join("collections/default"));
    let first = store.catalog()?.unwrap();
    let first_files = &first.inventory.as_ref().unwrap().files;
    assert_eq!(first_files["build/include/beacon.h"].language, "cpp");
    assert_eq!(first_files["unrelated.h"].language, "c");
    let initial = first_files["build/include/beacon.h"].state.clone();
    let unaffected = first_files["unrelated.h"].state.clone();
    let second=br#"[{"directory":".","file":"include/beacon.h","arguments":["unexecuted-compiler","-DORIGINAL=1","-x","c++","include/beacon.h"]}]"#;
    fs::write(&configuration, second)?;
    kernel.index()?;
    let changed = store.catalog()?.unwrap();
    let files = &changed.inventory.as_ref().unwrap().files;
    assert_ne!(
        files["build/include/beacon.h"].state.profile_id,
        initial.profile_id
    );
    assert_eq!(
        files["build/include/beacon.h"].state.source_hash,
        initial.source_hash
    );
    assert_eq!(files["unrelated.h"].state, unaffected);
    let explicit = serde_json::from_value(serde_json::json!({"version":2,"rules":[
        {"selector":{"kind":"header_context","prefix":"build/include","configuration":"build/compile_commands.json"},"case_sensitive":true,"adapter_id":"cpp"},
        {"selector":{"kind":"exact","path":"build/include/beacon.h"},"case_sensitive":true,"adapter_id":"c"}
    ]}))?;
    kernel.configure_extraction_policy(explicit)?;
    kernel.index()?;
    assert_eq!(
        store.catalog()?.unwrap().inventory.as_ref().unwrap().files["build/include/beacon.h"]
            .language,
        "c"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("build/include/beacon.h"))?,
        source
    );
    Ok(())
}
