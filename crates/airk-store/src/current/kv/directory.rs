// SPDX-License-Identifier: Apache-2.0
//! Authenticated ordered pages. Every traversal verifies ranges and counts from
//! its trusted parent; redb range iteration never establishes completeness.
use super::tables;
use crate::current::CurrentScope;
use airk_core::identity::{Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

pub const MAX_PAGE_BYTES: usize = 65_536;
pub const MAX_KEY_BYTES: usize = 8192;
pub const MAX_VALUE_BYTES: usize = 16_384;
const FANOUT: usize = 128;
const MAX_HEIGHT: u8 = 16;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub digest: Sha256Digest,
    pub first: Vec<u8>,
    pub last: Vec<u8>,
    pub count: u64,
    pub height: u8,
}
impl Root {
    pub fn validate(&self) -> Result<()> {
        root_valid(self)
    }
}
pub type Entry = (Vec<u8>, Vec<u8>);
enum Page {
    Leaf(Vec<Entry>),
    Branch(Vec<Root>),
}

/// One domain per logical tree, including the posting term or forward path ID.
/// The scope and domain are part of every page digest, preventing substitutions.
pub struct Tree {
    domain: Sha256Digest,
}
pub trait ReadPages {
    /// Enforce the encoded page bound before copying bytes from the engine.
    fn read_page(&self, digest: Sha256Digest) -> Result<Vec<u8>>;
}
pub trait WritePages: ReadPages {
    fn write_page(&self, digest: Sha256Digest, bytes: &[u8]) -> Result<()>;
    /// Remove the superseded reference in this transaction. Older engine read
    /// snapshots retain their own versions. Immutable planning stores may keep
    /// old pages until their captured operation set is sealed.
    fn retire_page(&self, _digest: Sha256Digest) -> Result<()> {
        Ok(())
    }
}
macro_rules! read_pages {
    ($transaction:ty) => {
        impl ReadPages for $transaction {
            fn read_page(&self, digest: Sha256Digest) -> Result<Vec<u8>> {
                let table = self.open_table(tables::LEX_DIRECTORY)?;
                let row = table
                    .get(digest.as_bytes().as_slice())?
                    .context("authenticated directory page missing")?;
                ensure!(
                    row.value().len() <= MAX_PAGE_BYTES,
                    "oversized directory page"
                );
                Ok(row.value().to_vec())
            }
        }
    };
}
read_pages!(redb::ReadTransaction);
read_pages!(redb::WriteTransaction);
impl WritePages for redb::WriteTransaction {
    fn retire_page(&self, digest: Sha256Digest) -> Result<()> {
        self.open_table(tables::LEX_DIRECTORY)?
            .remove(digest.as_bytes().as_slice())?;
        Ok(())
    }
    fn write_page(&self, digest: Sha256Digest, bytes: &[u8]) -> Result<()> {
        ensure!(
            bytes.len() <= MAX_PAGE_BYTES,
            "directory write exceeds page bound"
        );
        self.open_table(tables::LEX_DIRECTORY)?
            .insert(digest.as_bytes().as_slice(), bytes)?;
        Ok(())
    }
}

/// Shared by all directory walks in one query/publication. The caller supplies
/// stricter operation-wide budgets and cancellation through the visitor too.
pub struct Budget {
    pub pages: u64,
    pub bytes: u64,
    pub entries: u64,
}
#[derive(Debug)]
pub struct Exhausted(pub &'static str);
impl std::fmt::Display for Exhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "authenticated directory {} budget exhausted", self.0)
    }
}
impl std::error::Error for Exhausted {}
impl Budget {
    pub fn publication() -> Self {
        Self {
            pages: 1_048_576,
            bytes: 8 << 30,
            entries: 64 << 20,
        }
    }
    pub fn query() -> Self {
        Self {
            pages: 16_384,
            bytes: 64 << 20,
            entries: 1_048_576,
        }
    }
    pub(crate) fn page(&mut self) -> Result<()> {
        self.admit_page()?;
        self.pages = self.pages.checked_sub(1).ok_or(Exhausted("page"))?;
        // Admit the maximum before asking storage to materialize a page.
        ensure!(self.bytes >= MAX_PAGE_BYTES as u64, Exhausted("byte"));
        Ok(())
    }
    pub(crate) fn admit_page(&self) -> Result<()> {
        ensure!(self.pages > 0, Exhausted("page"));
        ensure!(self.bytes >= MAX_PAGE_BYTES as u64, Exhausted("byte"));
        Ok(())
    }
    fn entry(&mut self) -> Result<()> {
        self.entries = self.entries.checked_sub(1).ok_or(Exhausted("entry"))?;
        Ok(())
    }
}

fn key(key: &[u8]) -> Result<()> {
    ensure!(
        !key.is_empty() && key.len() <= MAX_KEY_BYTES,
        "invalid directory key"
    );
    Ok(())
}
fn root_valid(root: &Root) -> Result<()> {
    key(&root.first)?;
    key(&root.last)?;
    ensure!(
        root.first <= root.last && root.count > 0 && root.height <= MAX_HEIGHT,
        "invalid directory reference"
    );
    Ok(())
}
fn bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value);
}
fn encode(page: &Page) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    match page {
        Page::Leaf(entries) => {
            ensure!(
                !entries.is_empty() && entries.len() <= FANOUT,
                "invalid directory leaf count"
            );
            out.push(0);
            out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
            for (i, (k, v)) in entries.iter().enumerate() {
                key(k)?;
                ensure!(
                    v.len() <= MAX_VALUE_BYTES && (i == 0 || entries[i - 1].0 < *k),
                    "invalid directory leaf order/value"
                );
                bytes(&mut out, k);
                bytes(&mut out, v);
            }
        }
        Page::Branch(children) => {
            ensure!(
                !children.is_empty() && children.len() <= FANOUT,
                "invalid directory branch count"
            );
            out.push(1);
            out.extend_from_slice(&(children.len() as u16).to_be_bytes());
            for (i, child) in children.iter().enumerate() {
                root_valid(child)?;
                ensure!(
                    child.height < MAX_HEIGHT
                        && (i == 0
                            || (children[i - 1].last < child.first
                                && children[i - 1].height == child.height)),
                    "invalid directory child range/height"
                );
                out.extend_from_slice(child.digest.as_bytes());
                out.push(child.height);
                out.extend_from_slice(&child.count.to_be_bytes());
                bytes(&mut out, &child.first);
                bytes(&mut out, &child.last);
            }
        }
    }
    Ok(out)
}
struct Decoder<'a>(&'a [u8]);
impl<'a> Decoder<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        ensure!(n <= self.0.len(), "truncated directory page");
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn field(&mut self, maximum: usize) -> Result<Vec<u8>> {
        let n = u32::from_be_bytes(self.take(4)?.try_into()?) as usize;
        ensure!(n <= maximum, "oversized directory field");
        Ok(self.take(n)?.to_vec())
    }
}
fn decode(input: &[u8]) -> Result<Page> {
    ensure!(input.len() <= MAX_PAGE_BYTES, "oversized directory page");
    let mut d = Decoder(input);
    let tag = d.take(1)?[0];
    let n = usize::from(u16::from_be_bytes(d.take(2)?.try_into()?));
    ensure!((1..=FANOUT).contains(&n), "invalid directory page count");
    let page = match tag {
        0 => {
            let mut entries = Vec::with_capacity(n);
            for _ in 0..n {
                entries.push((d.field(MAX_KEY_BYTES)?, d.field(MAX_VALUE_BYTES)?));
            }
            Page::Leaf(entries)
        }
        1 => {
            let mut children = Vec::with_capacity(n);
            for _ in 0..n {
                children.push(Root {
                    digest: Sha256Digest::from_bytes(d.take(32)?.try_into()?),
                    height: d.take(1)?[0],
                    count: u64::from_be_bytes(d.take(8)?.try_into()?),
                    first: d.field(MAX_KEY_BYTES)?,
                    last: d.field(MAX_KEY_BYTES)?,
                });
            }
            Page::Branch(children)
        }
        _ => anyhow::bail!("unknown directory page tag"),
    };
    ensure!(
        d.0.is_empty() && encode(&page)? == input,
        "noncanonical directory page"
    );
    Ok(page)
}
impl Tree {
    pub(crate) fn payload_digest(&self, bytes: &[u8]) -> Sha256Digest {
        framed_digest(
            b"airk-lexical-directory-payload/1",
            &[self.domain.as_bytes(), bytes],
        )
    }
    /// Build sorted entries with one bounded page buffer per level. The caller
    /// retires a previous tree separately, inside the same engine transaction.
    pub fn build(
        &self,
        storage: &impl WritePages,
        entries: impl IntoIterator<Item = Entry>,
        budget: &mut Budget,
    ) -> Result<Option<Root>> {
        let mut leaf = Vec::new();
        let mut levels: Vec<Vec<Root>> = Vec::new();
        let mut previous: Option<Vec<u8>> = None;
        for (k, v) in entries {
            budget.entry()?;
            key(&k)?;
            ensure!(
                v.len() <= MAX_VALUE_BYTES && previous.as_ref().is_none_or(|p| p < &k),
                "invalid directory build order/value"
            );
            previous = Some(k.clone());
            leaf.push((k, v));
            if leaf.len() > FANOUT || encode(&Page::Leaf(leaf.clone()))?.len() > MAX_PAGE_BYTES {
                let last = leaf.pop().unwrap();
                let root = self.save(storage, Page::Leaf(std::mem::take(&mut leaf)))?;
                self.push_level(storage, &mut levels, 0, root)?;
                leaf.push(last);
            }
        }
        if !leaf.is_empty() {
            let root = self.save(storage, Page::Leaf(leaf))?;
            self.push_level(storage, &mut levels, 0, root)?;
        }
        let mut i = 0;
        while i < levels.len() {
            let roots = std::mem::take(&mut levels[i]);
            if !roots.is_empty() {
                if i + 1 == levels.len() && roots.len() == 1 {
                    return Ok(roots.into_iter().next());
                }
                let root = self.save(storage, Page::Branch(roots))?;
                self.push_level(storage, &mut levels, i + 1, root)?;
            }
            i += 1;
        }
        Ok(None)
    }
    fn push_level(
        &self,
        storage: &impl WritePages,
        levels: &mut Vec<Vec<Root>>,
        level: usize,
        root: Root,
    ) -> Result<()> {
        ensure!(
            level <= usize::from(MAX_HEIGHT),
            "directory build height bound exceeded"
        );
        if levels.len() <= level {
            levels.push(Vec::new());
        }
        levels[level].push(root);
        if levels[level].len() > FANOUT
            || encode(&Page::Branch(levels[level].clone()))?.len() > MAX_PAGE_BYTES
        {
            let last = levels[level].pop().unwrap();
            let branch = self.save(storage, Page::Branch(std::mem::take(&mut levels[level])))?;
            self.push_level(storage, levels, level + 1, branch)?;
            levels[level].push(last);
        }
        Ok(())
    }
    pub fn new(scope: &CurrentScope, domain: &[u8]) -> Result<Self> {
        scope.validate()?;
        ensure!(
            !domain.is_empty() && domain.len() <= 8192,
            "invalid directory domain"
        );
        Ok(Self {
            domain: framed_digest(
                b"airk-lexical-directory-domain/1",
                &[
                    scope.root.as_bytes(),
                    scope.collection.as_bytes(),
                    scope.incarnation.as_bytes(),
                    domain,
                ],
            ),
        })
    }
    pub fn empty_digest(&self) -> Sha256Digest {
        framed_digest(b"airk-lexical-directory-empty/1", &[self.domain.as_bytes()])
    }
    fn describe(&self, page: &Page, encoded: &[u8]) -> Result<Root> {
        let (first, last, count, height) = match page {
            Page::Leaf(entries) => (
                &entries[0].0,
                &entries[entries.len() - 1].0,
                entries.len() as u64,
                0,
            ),
            Page::Branch(children) => (
                &children[0].first,
                &children[children.len() - 1].last,
                children.iter().try_fold(0u64, |n, c| {
                    n.checked_add(c.count).context("directory count overflow")
                })?,
                children[0].height + 1,
            ),
        };
        Ok(Root {
            digest: framed_digest(
                b"airk-lexical-directory-page/1",
                &[self.domain.as_bytes(), encoded],
            ),
            first: first.clone(),
            last: last.clone(),
            count,
            height,
        })
    }
    fn read(&self, storage: &impl ReadPages, root: &Root, budget: &mut Budget) -> Result<Page> {
        root_valid(root)?;
        budget.page()?;
        let encoded = storage.read_page(root.digest)?;
        ensure!(
            encoded.len() <= MAX_PAGE_BYTES,
            "oversized directory storage result"
        );
        budget.bytes -= encoded.len() as u64;
        let page = decode(&encoded)?;
        ensure!(
            self.describe(&page, &encoded)? == *root,
            "directory digest/range/count mismatch"
        );
        Ok(page)
    }
    fn save(&self, storage: &impl WritePages, page: Page) -> Result<Root> {
        let encoded = encode(&page)?;
        ensure!(encoded.len() <= MAX_PAGE_BYTES, "directory page overflow");
        let root = self.describe(&page, &encoded)?;
        storage.write_page(root.digest, &encoded)?;
        Ok(root)
    }
    pub fn get(
        &self,
        storage: &impl ReadPages,
        root: Option<&Root>,
        key_bytes: &[u8],
        budget: &mut Budget,
    ) -> Result<Option<Vec<u8>>> {
        key(key_bytes)?;
        let Some(mut root) = root.cloned() else {
            return Ok(None);
        };
        loop {
            match self.read(storage, &root, budget)? {
                Page::Leaf(entries) => {
                    budget.entry()?;
                    return Ok(entries
                        .binary_search_by(|e| e.0.as_slice().cmp(key_bytes))
                        .ok()
                        .map(|i| entries[i].1.clone()));
                }
                Page::Branch(children) => {
                    let i = children.partition_point(|c| c.last.as_slice() < key_bytes);
                    if i == children.len() || key_bytes < children[i].first.as_slice() {
                        return Ok(None);
                    }
                    root = children[i].clone();
                }
            }
        }
    }
    /// Greatest authenticated key <= target, including proof of intervening
    /// gaps. This supports posting-block lookup without trusting an engine seek.
    pub fn floor(
        &self,
        storage: &impl ReadPages,
        root: Option<&Root>,
        target: &[u8],
        budget: &mut Budget,
    ) -> Result<Option<Entry>> {
        key(target)?;
        let Some(mut root) = root.cloned() else {
            return Ok(None);
        };
        loop {
            match self.read(storage, &root, budget)? {
                Page::Leaf(entries) => {
                    budget.entry()?;
                    let end = entries.partition_point(|e| e.0.as_slice() <= target);
                    return Ok(end.checked_sub(1).map(|i| entries[i].clone()));
                }
                Page::Branch(children) => {
                    let end = children.partition_point(|c| c.first.as_slice() <= target);
                    let Some(i) = end.checked_sub(1) else {
                        return Ok(None);
                    };
                    root = children[i].clone();
                }
            }
        }
    }
    pub fn ceiling(
        &self,
        storage: &impl ReadPages,
        root: Option<&Root>,
        target: &[u8],
        budget: &mut Budget,
    ) -> Result<Option<Entry>> {
        key(target)?;
        let Some(mut root) = root.cloned() else {
            return Ok(None);
        };
        loop {
            match self.read(storage, &root, budget)? {
                Page::Leaf(entries) => {
                    budget.entry()?;
                    let i = entries.partition_point(|e| e.0.as_slice() < target);
                    return Ok(entries.get(i).cloned());
                }
                Page::Branch(children) => {
                    let i = children.partition_point(|c| c.last.as_slice() < target);
                    let Some(child) = children.get(i) else {
                        return Ok(None);
                    };
                    root = child.clone();
                }
            }
        }
    }
    /// Half-open authenticated range. Descendant omission cannot turn into an
    /// empty result because every visited reference must resolve and verify.
    pub fn visit(
        &self,
        storage: &impl ReadPages,
        root: Option<&Root>,
        start: &[u8],
        end: Option<&[u8]>,
        budget: &mut Budget,
        visitor: &mut impl FnMut(&[u8], &[u8]) -> Result<()>,
    ) -> Result<()> {
        self.visit_budgeted(storage, root, start, end, budget, &mut |k, v, _| {
            visitor(k, v)
        })
    }
    pub fn visit_budgeted(
        &self,
        storage: &impl ReadPages,
        root: Option<&Root>,
        start: &[u8],
        end: Option<&[u8]>,
        budget: &mut Budget,
        visitor: &mut impl FnMut(&[u8], &[u8], &mut Budget) -> Result<()>,
    ) -> Result<()> {
        let Some(root) = root else {
            return Ok(());
        };
        // Verify the root even when the requested range lies outside it.
        self.visit_page(storage, root, start, end, budget, visitor)
    }
    fn visit_page(
        &self,
        storage: &impl ReadPages,
        root: &Root,
        start: &[u8],
        end: Option<&[u8]>,
        budget: &mut Budget,
        visitor: &mut impl FnMut(&[u8], &[u8], &mut Budget) -> Result<()>,
    ) -> Result<()> {
        match self.read(storage, root, budget)? {
            Page::Leaf(entries) => {
                for (k, v) in entries {
                    if k.as_slice() >= start && end.is_none_or(|e| k.as_slice() < e) {
                        budget.entry()?;
                        visitor(&k, &v, budget)?;
                    }
                }
            }
            Page::Branch(children) => {
                for child in children {
                    if child.last.as_slice() >= start
                        && end.is_none_or(|e| child.first.as_slice() < e)
                    {
                        self.visit_page(storage, &child, start, end, budget, visitor)?;
                    }
                }
            }
        }
        Ok(())
    }
    pub fn put(
        &self,
        storage: &impl WritePages,
        root: Option<&Root>,
        key_bytes: &[u8],
        value: Option<&[u8]>,
        budget: &mut Budget,
    ) -> Result<Option<Root>> {
        key(key_bytes)?;
        ensure!(
            value.is_none_or(|v| v.len() <= MAX_VALUE_BYTES),
            "directory value exceeds bound"
        );
        let mut roots = self.mutate(storage, root, key_bytes, value, budget)?;
        while roots.len() > 1 {
            roots = self.split_save(storage, Page::Branch(roots))?;
        }
        let mut root = roots.pop();
        // A root with one child is represented by that child.
        while let Some(r) = &root {
            if r.height == 0 {
                break;
            }
            match self.read(storage, r, budget)? {
                Page::Branch(mut children) if children.len() == 1 => {
                    storage.retire_page(r.digest)?;
                    root = children.pop();
                }
                _ => break,
            }
        }
        Ok(root)
    }
    pub fn clear(
        &self,
        storage: &impl WritePages,
        root: Option<&Root>,
        budget: &mut Budget,
    ) -> Result<()> {
        if let Some(root) = root {
            if let Page::Branch(children) = self.read(storage, root, budget)? {
                for child in children {
                    self.clear(storage, Some(&child), budget)?;
                }
            }
            storage.retire_page(root.digest)?;
        }
        Ok(())
    }
    fn mutate(
        &self,
        storage: &impl WritePages,
        root: Option<&Root>,
        k: &[u8],
        v: Option<&[u8]>,
        budget: &mut Budget,
    ) -> Result<Vec<Root>> {
        let next = self.mutate_page(storage, root, k, v, budget)?;
        if let Some(old) = root
            && !next.iter().any(|r| r.digest == old.digest)
        {
            storage.retire_page(old.digest)?;
        }
        Ok(next)
    }
    fn mutate_page(
        &self,
        storage: &impl WritePages,
        root: Option<&Root>,
        k: &[u8],
        v: Option<&[u8]>,
        budget: &mut Budget,
    ) -> Result<Vec<Root>> {
        let page = match root {
            Some(root) => self.read(storage, root, budget)?,
            None => Page::Leaf(vec![]),
        };
        let page = match page {
            Page::Leaf(mut entries) => {
                match (entries.binary_search_by(|e| e.0.as_slice().cmp(k)), v) {
                    (Ok(i), Some(v)) => entries[i].1 = v.to_vec(),
                    (Ok(i), None) => {
                        entries.remove(i);
                    }
                    (Err(i), Some(v)) => entries.insert(i, (k.to_vec(), v.to_vec())),
                    (Err(_), None) => {}
                }
                if entries.is_empty() {
                    return Ok(vec![]);
                }
                Page::Leaf(entries)
            }
            Page::Branch(mut children) => {
                let i = children
                    .partition_point(|c| c.last.as_slice() < k)
                    .min(children.len() - 1);
                let changed = self.mutate(storage, Some(&children[i]), k, v, budget)?;
                children.splice(i..i + 1, changed);
                if children.is_empty() {
                    return Ok(vec![]);
                }
                // Deterministic left-to-right merge of adjacent underfilled
                // pages. Coalesce only if both count and encoded bytes fit.
                let mut j = i.saturating_sub(1);
                while j + 1 < children.len() && j <= i {
                    let left = self.read(storage, &children[j], budget)?;
                    let right = self.read(storage, &children[j + 1], budget)?;
                    let merged = match (left, right) {
                        (Page::Leaf(mut a), Page::Leaf(b))
                            if a.len() <= FANOUT / 2 || b.len() <= FANOUT / 2 =>
                        {
                            a.extend(b);
                            Some(Page::Leaf(a))
                        }
                        (Page::Branch(mut a), Page::Branch(b))
                            if a.len() <= FANOUT / 2 || b.len() <= FANOUT / 2 =>
                        {
                            a.extend(b);
                            Some(Page::Branch(a))
                        }
                        _ => None,
                    };
                    if let Some(merged) =
                        merged.filter(|p| encode(p).is_ok_and(|b| b.len() <= MAX_PAGE_BYTES))
                    {
                        let r = self.save(storage, merged)?;
                        storage.retire_page(children[j].digest)?;
                        storage.retire_page(children[j + 1].digest)?;
                        children.splice(j..j + 2, [r]);
                    } else {
                        j += 1;
                    }
                }
                Page::Branch(children)
            }
        };
        if encode(&page).is_ok_and(|b| b.len() <= MAX_PAGE_BYTES) {
            return Ok(vec![self.save(storage, page)?]);
        }
        let (left, right) = match page {
            Page::Leaf(mut e) => {
                let r = e.split_off(e.len() / 2);
                (Page::Leaf(e), Page::Leaf(r))
            }
            Page::Branch(mut e) => {
                let r = e.split_off(e.len() / 2);
                (Page::Branch(e), Page::Branch(r))
            }
        };
        let mut roots = self.split_save(storage, left)?;
        roots.extend(self.split_save(storage, right)?);
        Ok(roots)
    }
    fn split_save(&self, storage: &impl WritePages, page: Page) -> Result<Vec<Root>> {
        if encode(&page).is_ok_and(|b| b.len() <= MAX_PAGE_BYTES) {
            return Ok(vec![self.save(storage, page)?]);
        }
        let (left, right) = match page {
            Page::Leaf(mut entries) => {
                ensure!(entries.len() > 1, "directory entry cannot fit page");
                let right = entries.split_off(entries.len() / 2);
                (Page::Leaf(entries), Page::Leaf(right))
            }
            Page::Branch(mut children) => {
                ensure!(children.len() > 1, "directory child cannot fit page");
                let right = children.split_off(children.len() / 2);
                (Page::Branch(children), Page::Branch(right))
            }
        };
        let mut roots = self.split_save(storage, left)?;
        roots.extend(self.split_save(storage, right)?);
        Ok(roots)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::identity::Incarnation;
    use std::{cell::RefCell, collections::BTreeMap};
    #[derive(Default)]
    struct Memory(RefCell<BTreeMap<Sha256Digest, Vec<u8>>>);
    impl ReadPages for Memory {
        fn read_page(&self, d: Sha256Digest) -> Result<Vec<u8>> {
            self.0
                .borrow()
                .get(&d)
                .cloned()
                .context("missing test page")
        }
    }
    impl WritePages for Memory {
        fn write_page(&self, d: Sha256Digest, b: &[u8]) -> Result<()> {
            self.0.borrow_mut().insert(d, b.to_vec());
            Ok(())
        }
    }
    fn tree(domain: &[u8]) -> Result<Tree> {
        Tree::new(
            &CurrentScope {
                root: "fixture/lexical".into(),
                collection: "lexical".into(),
                incarnation: Incarnation::from_bytes([3; 16]),
            },
            domain,
        )
    }
    #[test]
    fn bulk_build_and_transactional_page_retirement_preserve_pinned_reads() -> Result<()> {
        use redb::ReadableTableMetadata;
        let file = tempfile::NamedTempFile::new()?;
        let database = super::super::Database::open(file.reopen()?, 32 << 20, true)?;
        let tree = tree(b"bulk")?;
        let mut budget = Budget {
            pages: 1_000_000,
            bytes: 16 << 30,
            entries: 1_000_000,
        };
        let tx = database.write()?;
        let root = tree.build(
            &tx,
            (0..20_000u32).map(|i| (i.to_be_bytes().to_vec(), i.to_be_bytes().to_vec())),
            &mut budget,
        )?;
        tx.commit()?;
        let pinned = database.snapshot()?;
        let tx = database.write()?;
        let mut current = root.clone();
        for i in 0..500u32 {
            current = tree.put(
                &tx,
                current.as_ref(),
                &i.to_be_bytes(),
                Some(b"updated"),
                &mut budget,
            )?;
        }
        assert!(
            tx.open_table(tables::LEX_DIRECTORY)?.len()? < 400,
            "superseded pages leaked"
        );
        tx.commit()?;
        assert_eq!(
            tree.get(
                pinned.transaction(),
                root.as_ref(),
                &1u32.to_be_bytes(),
                &mut budget
            )?,
            Some(1u32.to_be_bytes().to_vec())
        );
        assert_eq!(
            tree.get(
                database.snapshot()?.transaction(),
                current.as_ref(),
                &1u32.to_be_bytes(),
                &mut budget
            )?,
            Some(b"updated".to_vec())
        );
        let tx = database.write()?;
        tree.clear(&tx, current.as_ref(), &mut budget)?;
        assert_eq!(tx.open_table(tables::LEX_DIRECTORY)?.len()?, 0);
        tx.commit()?;
        assert!(
            tree.get(
                pinned.transaction(),
                root.as_ref(),
                &19_999u32.to_be_bytes(),
                &mut budget
            )?
            .is_some()
        );
        Ok(())
    }
    #[test]
    fn generated_replacements_deletions_ranges_and_old_roots_match_ordered_map() -> Result<()> {
        let storage = Memory::default();
        let tree = tree(b"paths")?;
        let mut root = None;
        let mut reference = BTreeMap::new();
        let mut budget = Budget {
            pages: 1_000_000,
            bytes: 1 << 34,
            entries: 1_000_000,
        };
        for n in 0..1200u32 {
            let k = ((n * 337) % 479).to_be_bytes().to_vec();
            let v = (n % 7 != 0).then(|| n.to_be_bytes().to_vec());
            root = tree.put(&storage, root.as_ref(), &k, v.as_deref(), &mut budget)?;
            if let Some(v) = v {
                reference.insert(k, v);
            } else {
                reference.remove(&k);
            }
        }
        let old = root.clone();
        let mut actual = BTreeMap::new();
        tree.visit(
            &storage,
            root.as_ref(),
            &[],
            None,
            &mut budget,
            &mut |k, v| {
                actual.insert(k.to_vec(), v.to_vec());
                Ok(())
            },
        )?;
        assert_eq!(actual, reference);
        let mut range = BTreeMap::new();
        tree.visit(
            &storage,
            root.as_ref(),
            &123u32.to_be_bytes(),
            Some(&234u32.to_be_bytes()),
            &mut budget,
            &mut |k, v| {
                range.insert(k.to_vec(), v.to_vec());
                Ok(())
            },
        )?;
        assert_eq!(
            range,
            reference
                .range(123u32.to_be_bytes().to_vec()..234u32.to_be_bytes().to_vec())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        );
        for (k, v) in reference {
            assert_eq!(tree.get(&storage, old.as_ref(), &k, &mut budget)?, Some(v));
            root = tree.put(&storage, root.as_ref(), &k, None, &mut budget)?;
        }
        assert!(root.is_none());
        assert!(
            tree.get(&storage, old.as_ref(), &300u32.to_be_bytes(), &mut budget)?
                .is_some()
        );
        Ok(())
    }
    #[test]
    fn corrupt_missing_foreign_pages_and_forged_ranges_fail_closed() -> Result<()> {
        let storage = Memory::default();
        let tree = tree(b"terms")?;
        let mut budget = Budget::query();
        let root = tree
            .put(&storage, None, b"hello", Some(b"header"), &mut budget)?
            .unwrap();
        let mut changed = root.clone();
        changed.count += 1;
        assert!(
            tree.get(&storage, Some(&changed), b"hello", &mut budget)
                .is_err()
        );
        assert!(
            super::tests::tree(b"paths")?
                .get(&storage, Some(&root), b"hello", &mut budget)
                .is_err()
        );
        let bytes = storage.0.borrow_mut().remove(&root.digest).unwrap();
        assert!(
            tree.get(&storage, Some(&root), b"absent", &mut budget)
                .is_err()
        );
        let mut changed = bytes;
        *changed.last_mut().unwrap() ^= 1;
        storage.0.borrow_mut().insert(root.digest, changed);
        assert!(
            tree.get(&storage, Some(&root), b"hello", &mut budget)
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn resource_exhaustion_is_typed_and_never_a_partial_success() -> Result<()> {
        let storage = Memory::default();
        let tree = tree(b"documents")?;
        let root = tree.put(&storage, None, b"key", Some(b"value"), &mut Budget::query())?;
        for mut budget in [
            Budget {
                pages: 0,
                bytes: 1 << 20,
                entries: 1,
            },
            Budget {
                pages: 1,
                bytes: 0,
                entries: 1,
            },
            Budget {
                pages: 1,
                bytes: 1 << 20,
                entries: 0,
            },
        ] {
            assert!(
                tree.get(&storage, root.as_ref(), b"key", &mut budget)
                    .unwrap_err()
                    .is::<Exhausted>()
            );
        }
        Ok(())
    }
}
