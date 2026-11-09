// SPDX-License-Identifier: Apache-2.0
//! Immutable keyed selectors, reachable only through their owning projection.
use super::*;
use crate::current::postings::Lookup;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct MetadataBudget {
    pub(super) remaining: usize,
    pub(super) objects: usize,
}
impl MetadataBudget {
    pub(super) fn new(bytes: usize) -> Self {
        Self {
            remaining: bytes,
            objects: 0,
        }
    }
    pub(super) fn charge(&mut self, reference: &ResourceReference) -> Result<()> {
        ensure!(
            reference.projection.is_none(),
            "nested projection admission summary"
        );
        self.remaining = self
            .remaining
            .checked_sub(reference.bytes)
            .context("projection metadata budget exceeded")?;
        self.objects = self
            .objects
            .checked_add(1)
            .context("projection metadata count overflow")?;
        Ok(())
    }
    pub(super) fn finish(&self, header: &ProjectionHeader) -> Result<()> {
        ensure!(
            self.remaining == 0 && self.objects == header.metadata_objects,
            "projection metadata declaration mismatch"
        );
        Ok(())
    }
}

const INLINE_BYTES: usize = 4096;
const LEAF_KEYS: usize = 64;
const MAX_SELECTOR_MEMORY: usize = 32 << 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedPath {
    pub path: String,
    pub source: Sha256Digest,
    pub pages: Vec<usize>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Value {
    Inline { paths: Vec<SelectedPath> },
    Blocks { reference: ResourceReference },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    key: Sha256Digest,
    value: Value,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Node {
    Leaf {
        version: u32,
        depth: usize,
        entries: Vec<Entry>,
    },
    Branch {
        version: u32,
        depth: usize,
        keys: usize,
        children: Vec<Option<ResourceReference>>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathBlock {
    first: String,
    last: String,
    reference: ResourceReference,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathBlocks {
    version: u32,
    key: Sha256Digest,
    paths: usize,
    blocks: Vec<PathBlock>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Paths {
    version: u32,
    key: Sha256Digest,
    paths: Vec<SelectedPath>,
}

#[derive(Default)]
pub(super) struct Selectors {
    values: BTreeMap<Sha256Digest, BTreeMap<String, SelectedPath>>,
    memory: usize,
}
impl Selectors {
    fn add(&mut self, selector: Lookup<'_>, path: &str, source: &str, page: usize) -> Result<()> {
        let key = selector.identity();
        if !self.values.contains_key(&key) {
            self.memory = self.memory.saturating_add(256);
        }
        let paths = self.values.entry(key).or_default();
        if !paths.contains_key(path) {
            self.memory = self.memory.saturating_add(path.len() + 512);
        }
        let entry = paths.entry(path.into()).or_insert_with(|| SelectedPath {
            path: path.into(),
            source: Sha256Digest::from_bytes([0; 32]),
            pages: vec![],
        });
        let hash = source.parse()?;
        if entry.pages.is_empty() {
            entry.source = hash;
        }
        ensure!(
            entry.source == hash,
            "projection selector source disagreement"
        );
        if entry.pages.last() != Some(&page) {
            self.memory = self.memory.saturating_add(96);
            entry.pages.push(page);
        }
        if self.memory > MAX_SELECTOR_MEMORY {
            return Err(ProjectionPreparationLimit.into());
        }
        Ok(())
    }
    pub(super) fn memory_bytes(&self) -> usize {
        self.memory
    }
    pub(super) fn fact(&mut self, hit: &FactHit, language: &str, page: usize) -> Result<()> {
        let path = &hit.citation.path;
        let source = &hit.citation.content_hash;
        self.add(Lookup::Files, path, source, page)?;
        self.add(Lookup::FactKind(&hit.kind), path, source, page)?;
        self.add(Lookup::Language(language), path, source, page)?;
        let mut names = BTreeSet::from([hit.name.as_str()]);
        if let Some(code) = &hit.code {
            names.insert(&code.qualified_name);
            names.insert(&code.symbol_id);
            if let Some(name) = airk_core::code::terminal_call_name(code, &hit.kind) {
                names.insert(name);
            }
        }
        for name in names {
            self.add(Lookup::FactName(name), path, source, page)?;
        }
        Ok(())
    }
    pub(super) fn edge(&mut self, hit: &RelationshipHit, page: usize) -> Result<()> {
        self.add(
            Lookup::EdgeSource(&hit.source),
            &hit.citation.path,
            &hit.citation.content_hash,
            page,
        )?;
        self.add(
            Lookup::EdgeTarget(&hit.target),
            &hit.citation.path,
            &hit.citation.content_hash,
            page,
        )
    }
    pub(super) fn finish(
        self,
        builder: &mut Builder<'_>,
    ) -> Result<(Option<ResourceReference>, usize)> {
        let count = self.values.len();
        let mut entries = Vec::with_capacity(count);
        for (key, values) in self.values {
            let paths = values.into_values().collect::<Vec<_>>();
            let inline = Value::Inline { paths };
            let value = if bounded_encoding(&inline)?.len() <= INLINE_BYTES {
                inline
            } else {
                let Value::Inline { paths } = inline else {
                    unreachable!()
                };
                let mut blocks = Vec::new();
                let mut group = Vec::new();
                let mut used = 0usize;
                let count = paths.len();
                for path in paths {
                    let bytes = bounded_encoding(&path)?.len();
                    if !group.is_empty() && (group.len() == ROWS || used + bytes + 128 > PAGE_BYTES)
                    {
                        blocks.push(retain_block(builder, key, std::mem::take(&mut group))?);
                        used = 0;
                    }
                    used += bytes + 1;
                    group.push(path);
                }
                if !group.is_empty() {
                    blocks.push(retain_block(builder, key, group)?);
                }
                let bytes = bounded_encoding(&PathBlocks {
                    version: 1,
                    key,
                    paths: count,
                    blocks,
                })?;
                if bytes.len() > MAX_HEADER {
                    return Err(ProjectionPreparationLimit.into());
                }
                Value::Blocks {
                    reference: builder.resource(bytes)?,
                }
            };
            entries.push(Entry { key, value });
        }
        let root = if entries.is_empty() {
            None
        } else {
            Some(retain_node(builder, entries, 0)?)
        };
        Ok((root, count))
    }

    pub(super) fn verify(
        &self,
        store: &FileStore,
        header: &ProjectionHeader,
        budget: &mut MetadataBudget,
    ) -> Result<()> {
        ensure!(
            header.selector_keys == self.values.len(),
            "projection selector coverage mismatch"
        );
        let Some(root) = &header.selectors else {
            ensure!(self.values.is_empty(), "projection selectors omitted");
            return Ok(());
        };
        let actual_keys = walk(store, root, &[], budget, &mut |_, node, remaining| {
            if let Node::Leaf { entries, .. } = node {
                for entry in entries {
                    let expected = self
                        .values
                        .get(&entry.key)
                        .context("extra projection selector")?;
                    let actual = value_paths(
                        store,
                        entry.key,
                        &entry.value,
                        header.pages,
                        "",
                        None,
                        100_000,
                        Some(expected.len()),
                        None,
                        remaining,
                    )?;
                    ensure!(
                        actual.len() == expected.len() && actual.iter().eq(expected.values()),
                        "projection selector path/page membership mismatch"
                    );
                }
            }
            Ok(())
        })?;
        ensure!(
            actual_keys == self.values.len(),
            "projection selector tree membership mismatch"
        );
        Ok(())
    }
}

fn retain_block(
    builder: &mut Builder<'_>,
    key: Sha256Digest,
    paths: Vec<SelectedPath>,
) -> Result<PathBlock> {
    let first = paths.first().context("empty selector block")?.path.clone();
    let last = paths.last().context("empty selector block")?.path.clone();
    let bytes = bounded_encoding(&Paths {
        version: 1,
        key,
        paths,
    })?;
    if bytes.len() > PAGE_BYTES {
        return Err(ProjectionPreparationLimit.into());
    }
    Ok(PathBlock {
        first,
        last,
        reference: builder.resource(bytes)?,
    })
}
fn nibble(key: Sha256Digest, depth: usize) -> usize {
    let byte = key.as_bytes()[depth / 2];
    usize::from(if depth.is_multiple_of(2) {
        byte >> 4
    } else {
        byte & 15
    })
}
fn retain_node(
    builder: &mut Builder<'_>,
    entries: Vec<Entry>,
    depth: usize,
) -> Result<ResourceReference> {
    if entries.len() <= LEAF_KEYS {
        let leaf = Node::Leaf {
            version: 1,
            depth,
            entries: entries.clone(),
        };
        let bytes = bounded_encoding(&leaf)?;
        if bytes.len() <= PAGE_BYTES {
            return builder.resource(bytes);
        }
    }
    if depth == 64 {
        return Err(ProjectionPreparationLimit.into());
    }
    let keys = entries.len();
    let mut groups: [Vec<Entry>; 16] = std::array::from_fn(|_| vec![]);
    for entry in entries {
        groups[nibble(entry.key, depth)].push(entry);
    }
    let mut children = Vec::with_capacity(16);
    for group in groups {
        children.push(if group.is_empty() {
            None
        } else {
            Some(retain_node(builder, group, depth + 1)?)
        });
    }
    builder.resource(bounded_encoding(&Node::Branch {
        version: 1,
        depth,
        keys,
        children,
    })?)
}
fn node(store: &FileStore, reference: &ResourceReference, prefix: &[usize]) -> Result<Node> {
    ensure!(
        reference.bytes <= PAGE_BYTES && prefix.len() <= 64,
        "selector node bound exceeded"
    );
    let bytes = store.read_current_resource(reference)?;
    let node: Node = serde_json::from_slice(&bytes)?;
    ensure!(
        bounded_encoding(&node)? == bytes,
        "noncanonical selector node"
    );
    match &node {
        Node::Leaf {
            version,
            depth,
            entries,
        } => ensure!(
            *version == 1
                && *depth == prefix.len()
                && !entries.is_empty()
                && entries.len() <= LEAF_KEYS
                && entries.windows(2).all(|pair| pair[0].key < pair[1].key)
                && entries.iter().all(|entry| prefix
                    .iter()
                    .enumerate()
                    .all(|(depth, part)| nibble(entry.key, depth) == *part)),
            "invalid selector leaf"
        ),
        Node::Branch {
            version,
            depth,
            keys,
            children,
        } => ensure!(
            *version == 1
                && *depth == prefix.len()
                && *depth < 64
                && *keys > 0
                && *keys <= 2_000_000
                && children.len() == 16
                && children.iter().any(Option::is_some),
            "invalid selector branch"
        ),
    }
    Ok(node)
}
fn lookup(store: &FileStore, root: &ResourceReference, key: Sha256Digest) -> Result<Option<Value>> {
    let mut reference = root.clone();
    let mut prefix = Vec::new();
    loop {
        match node(store, &reference, &prefix)? {
            Node::Leaf { entries, .. } => {
                return Ok(entries
                    .into_iter()
                    .find(|entry| entry.key == key)
                    .map(|entry| entry.value));
            }
            Node::Branch { children, .. } => {
                let part = nibble(key, prefix.len());
                let Some(child) = &children[part] else {
                    return Ok(None);
                };
                reference = child.clone();
                prefix.push(part);
            }
        }
    }
}
fn validate_paths(paths: &[SelectedPath], pages: usize) -> Result<()> {
    ensure!(
        !paths.is_empty()
            && paths.windows(2).all(|pair| pair[0].path < pair[1].path)
            && paths.iter().all(
                |path| crate::current::journal::valid_current_path(&path.path)
                    && !path.pages.is_empty()
                    && path.pages.windows(2).all(|pair| pair[0] < pair[1])
                    && path.pages.iter().all(|page| *page < pages)
            ),
        "invalid selected path/page list"
    );
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn value_paths(
    store: &FileStore,
    key: Sha256Digest,
    value: &Value,
    pages: usize,
    prefix: &str,
    after: Option<&str>,
    limit: usize,
    expected: Option<usize>,
    scope: Option<&airk_core::scoped_reads::CompiledReadScope>,
    remaining: &mut MetadataBudget,
) -> Result<Vec<SelectedPath>> {
    let selected = |path: &SelectedPath| {
        scope.is_none_or(|scope| scope.allows_path(&path.path))
            && path.path.starts_with(prefix)
            && after.is_none_or(|after| path.path.as_str() > after)
    };
    match value {
        Value::Inline { paths } => {
            ensure!(
                expected.is_none_or(|count| paths.len() == count),
                "selector inline coverage mismatch"
            );
            ensure!(
                bounded_encoding(value)?.len() <= INLINE_BYTES,
                "oversized inline selector"
            );
            validate_paths(paths, pages)?;
            Ok(paths
                .iter()
                .filter(|path| selected(path))
                .take(limit)
                .cloned()
                .collect())
        }
        Value::Blocks { reference } => {
            ensure!(
                reference.bytes <= MAX_HEADER,
                "selector header bound exceeded"
            );
            remaining.charge(reference)?;
            let bytes = store.read_current_resource(reference)?;
            let header: PathBlocks = serde_json::from_slice(&bytes)?;
            ensure!(
                header.version == 1
                    && header.key == key
                    && header.paths <= 100_000
                    && expected.is_none_or(|count| header.paths == count)
                    && !header.blocks.is_empty()
                    && header.blocks.len() <= header.paths
                    && header.blocks.iter().all(|block| block.first <= block.last)
                    && header
                        .blocks
                        .windows(2)
                        .all(|pair| pair[0].last < pair[1].first)
                    && bounded_encoding(&header)? == bytes,
                "invalid selector blocks"
            );
            let expected_paths = header.paths;
            let mut result = Vec::new();
            let mut visited_paths = 0usize;
            for block in header.blocks {
                if block.last.as_str() < prefix
                    || after.is_some_and(|after| block.last.as_str() <= after)
                {
                    continue;
                }
                if block.first.as_str() > prefix && !block.first.starts_with(prefix) {
                    break;
                }
                ensure!(
                    block.reference.bytes <= PAGE_BYTES,
                    "selector path block bound exceeded"
                );
                remaining.charge(&block.reference)?;
                let bytes = store.read_current_resource(&block.reference)?;
                let paths: Paths = serde_json::from_slice(&bytes)?;
                ensure!(
                    paths.version == 1
                        && paths.key == key
                        && !paths.paths.is_empty()
                        && paths.paths.len() <= ROWS
                        && paths
                            .paths
                            .first()
                            .is_some_and(|path| path.path == block.first)
                        && paths
                            .paths
                            .last()
                            .is_some_and(|path| path.path == block.last)
                        && bounded_encoding(&paths)? == bytes,
                    "invalid selector path block"
                );
                validate_paths(&paths.paths, pages)?;
                visited_paths = visited_paths
                    .checked_add(paths.paths.len())
                    .context("selector path count overflow")?;
                ensure!(
                    expected.is_none_or(|count| visited_paths <= count),
                    "extra selector path membership"
                );
                result.extend(
                    paths
                        .paths
                        .into_iter()
                        .filter(&selected)
                        .take(limit - result.len()),
                );
                if result.len() == limit && expected.is_none() {
                    break;
                }
            }
            ensure!(
                expected.is_none_or(|count| count == visited_paths),
                "selector path membership incomplete"
            );
            if scope.is_none() && prefix.is_empty() && after.is_none() && limit >= expected_paths {
                ensure!(
                    result.len() == expected_paths,
                    "selector path count mismatch"
                );
            }
            Ok(result)
        }
    }
}
fn walk(
    store: &FileStore,
    root: &ResourceReference,
    prefix: &[usize],
    remaining: &mut MetadataBudget,
    visit: &mut impl FnMut(&ResourceReference, &Node, &mut MetadataBudget) -> Result<()>,
) -> Result<usize> {
    remaining.charge(root)?;
    let value = node(store, root, prefix)?;
    visit(root, &value, remaining)?;
    match value {
        Node::Leaf { entries, .. } => Ok(entries.len()),
        Node::Branch { keys, children, .. } => {
            let mut actual = 0usize;
            for (part, child) in children.into_iter().enumerate() {
                if let Some(child) = child {
                    let mut prefix = prefix.to_vec();
                    prefix.push(part);
                    actual = actual
                        .checked_add(walk(store, &child, &prefix, remaining, visit)?)
                        .context("selector key count overflow")?;
                }
            }
            ensure!(actual == keys, "selector branch count mismatch");
            Ok(actual)
        }
    }
}

impl CurrentQueryProjection<'_> {
    /// Selected path metadata only; original query budgets remain the caller's contract.
    pub fn lookup_paths(
        &self,
        selector: Lookup<'_>,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SelectedPath>> {
        self.read.token()?;
        let selected = select_paths_scoped(
            self.store,
            &self.header,
            selector,
            prefix,
            after,
            limit,
            self.read.read_scope.get().map(AsRef::as_ref),
        )?;
        if self.read.read_scope_fingerprint().is_some() {
            self.read.token()?;
        }
        Ok(selected)
    }
}
#[cfg(test)]
pub(crate) fn select_paths(
    store: &FileStore,
    header: &ProjectionHeader,
    selector: Lookup<'_>,
    prefix: &str,
    after: Option<&str>,
    limit: usize,
) -> Result<Vec<SelectedPath>> {
    select_paths_scoped(store, header, selector, prefix, after, limit, None)
}
#[allow(clippy::too_many_arguments)]
fn select_paths_scoped(
    store: &FileStore,
    header: &ProjectionHeader,
    selector: Lookup<'_>,
    prefix: &str,
    after: Option<&str>,
    limit: usize,
    scope: Option<&airk_core::scoped_reads::CompiledReadScope>,
) -> Result<Vec<SelectedPath>> {
    ensure!(
        (1..=100_000).contains(&limit),
        "invalid projection selector limit"
    );
    let Some(root) = &header.selectors else {
        ensure!(
            header.selector_keys == 0,
            "projection selector root missing"
        );
        return Ok(vec![]);
    };
    let key = selector.identity();
    let Some(value) = lookup(store, root, key)? else {
        return Ok(vec![]);
    };
    value_paths(
        store,
        key,
        &value,
        header.pages,
        prefix,
        after,
        limit,
        None,
        scope,
        &mut MetadataBudget::new(MAX_BYTES),
    )
}

pub(super) fn retain_paths(
    store: &FileStore,
    header: &ProjectionHeader,
    retain: &mut impl FnMut(&ResourceReference) -> Result<()>,
    budget: &mut MetadataBudget,
) -> Result<()> {
    let Some(root) = &header.selectors else {
        ensure!(header.selector_keys == 0, "missing selector root");
        return Ok(());
    };
    let count = walk(
        store,
        root,
        &[],
        budget,
        &mut |reference, node, remaining| {
            retain(reference)?;
            if let Node::Leaf { entries, .. } = node {
                for entry in entries {
                    if let Value::Blocks { reference } = &entry.value {
                        retain(reference)?;
                        ensure!(
                            reference.bytes <= MAX_HEADER,
                            "selector ownership header bound exceeded"
                        );
                        remaining.charge(reference)?;
                        let bytes = store.read_current_resource(reference)?;
                        let paths: PathBlocks = serde_json::from_slice(&bytes)?;
                        ensure!(
                            paths.version == 1
                                && paths.key == entry.key
                                && paths.blocks.len() <= 100_000
                                && bounded_encoding(&paths)? == bytes,
                            "invalid selector ownership header"
                        );
                        for block in paths.blocks {
                            remaining.charge(&block.reference)?;
                            retain(&block.reference)?;
                        }
                    }
                }
            }
            Ok(())
        },
    )?;
    ensure!(
        count == header.selector_keys,
        "selector ownership key count mismatch"
    );
    Ok(())
}
