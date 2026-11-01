// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{evidence::EvidenceReadOptions, identity::framed_digest};

#[test]
fn strict_selected_lines_match_chunked_values_and_exact_conditional_preimages() -> Result<()> {
    let root = crate::unlinked_tempdir()?;
    let data = crate::unlinked_tempdir()?;
    let cases = [
        ("empty.txt", String::new()),
        ("lf.txt", "first\nsecond\n".into()),
        ("crlf.txt", "first\r\nsecond\r\n".into()),
        ("final.txt", "first\nlast".into()),
        ("unicode.txt", "aé界🦀\n\nlast🦀".into()),
        ("dense.txt", "\n".repeat(12_001)),
        ("long.txt", format!("{}🦀\nlast", "x".repeat(65_534))),
    ];
    // Plain text exercises legacy contract 1; HTML exercises actual contract-2
    // native source reconstruction, including partial/plain host syntax.
    let cases = cases
        .into_iter()
        .flat_map(|(path, source)| {
            [
                (path.to_owned(), source.clone()),
                (path.replace(".txt", ".html"), source),
            ]
        })
        .collect::<Vec<_>>();
    for (path, source) in &cases {
        std::fs::write(root.path().join(path), source)?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    for (path, source) in cases {
        let path = path.as_str();
        let read = kernel.current_reader()?;
        let compact = read.artifact(path)?.context("fixture missing")?;
        let artifact = kernel.current_artifact(&read, path, true)?;
        let mut metadata = kernel.empty_snapshot();
        metadata.generation = read.head().sequence;
        let index = read.native_line_projection(&kernel.store, path)?;
        if compact
            .retained
            .as_ref()
            .and_then(|retained| retained.profile.as_ref())
            .is_some_and(|profile| profile.contract_version == 2)
        {
            assert_eq!(
                index
                    .with_context(|| format!("producer omitted native line index: {path}"))?
                    .lines(),
                source.split_inclusive('\n').count()
            );
        } else {
            assert!(index.is_none());
        }
        let count = source.split_inclusive('\n').count();
        if count == 0 {
            assert!(kernel.read_file(path, 1, 1, 100).is_err());
            assert!(kernel.context(path, 1, 0, 0, 100).is_err());
            continue;
        }
        for (start, end) in [(1, 1), (1, count), (count, count)] {
            for budget in [1, 3, 7, 65_536] {
                let expected = Kernel::read_snapshot(
                    Some(envelope(&read)?),
                    &metadata,
                    &artifact,
                    path,
                    start,
                    end,
                    budget,
                )?;
                let actual = kernel.read_file(path, start, end, budget)?;
                assert_eq!(
                    serde_json::to_value(&actual)?,
                    serde_json::to_value(&expected)?,
                    "{path}:{start}:{end}:{budget}"
                );
                let conditional =
                    kernel.read_file_conditional(path, start, end, budget, &Default::default())?;
                let mut representation = serde_json::to_value(&expected)?;
                let fields = representation.as_object_mut().unwrap();
                fields.remove("publication");
                fields.remove("generation");
                let retained = compact.retained.as_ref().unwrap();
                let validator = framed_digest(
                    b"current-read-representation/1",
                    &[
                        read.head().scope.incarnation.as_bytes(),
                        path.as_bytes(),
                        compact.content_hash.as_bytes(),
                        retained.object.as_bytes(),
                        conditional
                            .read
                            .original_root
                            .as_deref()
                            .unwrap_or_default()
                            .as_bytes(),
                        &serde_json::to_vec(&("read_file/1", start, end, budget))?,
                        &serde_json::to_vec(&representation)?,
                    ],
                );
                assert_eq!(conditional.read.validator, Some(validator));
                assert_eq!(
                    serde_json::to_value(conditional.content)?,
                    serde_json::to_value(Some(expected))?
                );
                let unchanged = kernel.read_file_conditional(
                    path,
                    start,
                    end,
                    budget,
                    &EvidenceReadOptions {
                        if_evidence: Some(validator),
                        ..Default::default()
                    },
                )?;
                assert!(unchanged.content.is_none());
            }
        }
        assert!(kernel.read_file(path, 1, count + 1, 100).is_err());
        let ordinary = kernel.context(path, count, 1, 1, 7)?;
        let conditional = kernel.context_conditional(path, count, 1, 1, 7, &Default::default())?;
        assert_eq!(
            serde_json::to_value(ordinary)?,
            serde_json::to_value(conditional.content.unwrap())?
        );
        assert!(kernel.context(path, count + 1, 0, 0, 7).is_err());
    }
    Ok(())
}
