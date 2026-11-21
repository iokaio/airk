// SPDX-License-Identifier: Apache-2.0
//! Emit the canonical file pages of an existing publication without starting a service.

use airk_core::{
    discovery::DiscoveryOptions, identity::RevisionToken, service::ProjectRegistration,
};
use airk_kernel::Kernel;
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    io::{self, Write},
    time::Instant,
};

fn main() -> Result<()> {
    let [source, data, collection, binding, token, count]: [String; 6] = std::env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| {
            anyhow::anyhow!("expected SOURCE DATA COLLECTION BINDING REVISION FILE_COUNT")
        })?;
    let expected_count: usize = count.parse()?;
    let expected: RevisionToken = token.parse()?;
    let project = ProjectRegistration::new(source, data, collection)?;
    ensure!(
        project.binding.to_string() == binding,
        "project binding differs"
    );
    let start = Instant::now();
    let observer = Kernel::open_existing_service_observer(&project)?;
    eprintln!("observer_open_ms={}", start.elapsed().as_millis());
    // The normal activation path includes this exact storage observation in
    // its synchronous project inspection, before acknowledging activation.
    let inspection_start = Instant::now();
    let storage = observer.service_storage_summary()?;
    eprintln!(
        "native_storage_summary_ms={} storage={}",
        inspection_start.elapsed().as_millis(),
        serde_json::to_string(&storage)?
    );
    let mut options = DiscoveryOptions::default();
    let mut previous: Option<String> = None;
    let mut cursors = BTreeSet::new();
    let mut count = 0usize;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    loop {
        let page = observer.list_files_page("", 100, 64 << 10, &options)?;
        let publication = page.publication.as_ref().context("missing publication")?;
        ensure!(publication.token == expected, "publication changed");
        ensure!(page.collection == project.collection, "collection changed");
        for path in &page.paths {
            ensure!(
                previous.as_ref().is_none_or(|prior| prior < path),
                "duplicate or unordered native path"
            );
            previous = Some(path.clone());
        }
        count = count
            .checked_add(page.paths.len())
            .context("count overflow")?;
        ensure!(count <= expected_count, "unexpected extra native paths");
        serde_json::to_writer(&mut stdout, &page)?;
        writeln!(stdout)?;
        stdout.flush()?;
        if !page.truncated {
            ensure!(
                page.page.cursor.is_none() && count == expected_count,
                "incomplete census"
            );
            break;
        }
        ensure!(!page.paths.is_empty(), "empty continuation page");
        let next = page.page.cursor.context("truncated page has no cursor")?;
        ensure!(cursors.insert(next.clone()), "repeated native continuation");
        options.cursor = Some(next);
    }
    eprintln!(
        "native_files={count} elapsed_ms={}",
        start.elapsed().as_millis()
    );
    Ok(())
}
