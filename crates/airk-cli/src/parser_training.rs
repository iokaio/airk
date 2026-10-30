// SPDX-License-Identifier: Apache-2.0
//! Development-only native vocabulary export. Never included in the normal CLI build.
use airk_core::{FactQuery, discovery::DiscoveryOptions};
use airk_kernel::Kernel;
use anyhow::{Result, ensure};
use serde_json::json;
use std::{collections::BTreeSet, path::Path};

pub fn dump(root: &Path, data: &Path, output: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    ensure!(
        !std::path::absolute(data)?.starts_with(&root),
        "training index must be outside source root"
    );
    ensure!(
        !std::path::absolute(output)?.starts_with(&root),
        "vocabulary export must be outside source root"
    );
    let kernel = Kernel::open(&root, Some(data.to_path_buf()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let status = kernel.status_summary()?;
    let mut names = BTreeSet::new();
    let mut headings = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut truncated = false;
    for (kind, document_only, destination) in [
        ("definition", false, &mut names),
        ("section", true, &mut headings),
    ] {
        let mut cursor = None;
        for page in 0..500 {
            let response = kernel.facts_page(
                &FactQuery {
                    path_prefix: String::new(),
                    name: None,
                    kind: Some(kind.into()),
                    language: None,
                    exact_name: false,
                    limit: 100,
                    max_bytes: 65536,
                    path_exact: false,
                    document_only,
                },
                &DiscoveryOptions {
                    cursor,
                    minimal: false,
                    compact: false,
                },
            )?;
            for fact in response.facts {
                if !fact.name.is_empty() && fact.name.len() <= 256 {
                    destination.insert(fact.name);
                }
            }
            cursor = response.page.cursor;
            if cursor.is_none() {
                break;
            }
            if page == 499 {
                truncated = true;
            }
        }
    }
    let mut cursor = None;
    for page in 0..500 {
        let response = kernel.list_files_page(
            "",
            100,
            65536,
            &DiscoveryOptions {
                cursor,
                minimal: false,
                compact: false,
            },
        )?;
        paths.extend(response.paths);
        cursor = response.page.cursor;
        if cursor.is_none() {
            break;
        }
        if page == 499 {
            truncated = true;
        }
    }
    let bytes = serde_json::to_vec_pretty(
        &json!({"schema":1,"publication":status["publication"],"indexed_files":status["files"],"truncated":truncated,"symbols":names,"headings":headings,"paths":paths}),
    )?;
    ensure!(
        bytes.len() <= 8 * 1024 * 1024,
        "vocabulary exceeds eight MiB"
    );
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, bytes)?;
    println!(
        "{}",
        json!({"exported":output,"symbols":names.len(),"headings":headings.len(),"paths":paths.len(),"truncated":truncated})
    );
    Ok(())
}
