// SPDX-License-Identifier: Apache-2.0
//! Canonical sorted delta blocks. Directories authenticate every block's key,
//! full bytes and range; no unproved redb scan establishes posting completeness.
use super::directory::{Budget, ReadPages, Root, Tree, WritePages};
use super::tables;
use crate::current::CurrentScope;
use airk_core::identity::Sha256Digest;
use anyhow::{Context, Result, ensure};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

pub const MAX_BLOCK_KEYS: usize = 4096;
pub const MAX_BLOCK_BYTES: usize = 65_536;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub root: Option<Root>,
    pub count: u64,
}
pub trait ReadPostings: ReadPages {
    fn read_block(&self, key: &[u8; 40], budget: &mut Budget) -> Result<Vec<u8>>;
}
pub trait WritePostings: ReadPostings + WritePages {
    fn put_block(&self, key: &[u8; 40], bytes: Option<&[u8]>) -> Result<()>;
}
macro_rules! read_postings {
    ($transaction:ty) => {
        impl ReadPostings for $transaction {
            fn read_block(&self, key: &[u8; 40], budget: &mut Budget) -> Result<Vec<u8>> {
                budget.page()?;
                let table = self.open_table(tables::LEX_BLOCK)?;
                let row = table
                    .get(key.as_slice())?
                    .context("authenticated posting block missing")?;
                ensure!(
                    row.value().len() <= MAX_BLOCK_BYTES,
                    "oversized posting block"
                );
                budget.bytes -= row.value().len() as u64;
                Ok(row.value().to_vec())
            }
        }
    };
}
read_postings!(redb::ReadTransaction);
read_postings!(redb::WriteTransaction);
impl WritePostings for redb::WriteTransaction {
    fn put_block(&self, key: &[u8; 40], bytes: Option<&[u8]>) -> Result<()> {
        let mut table = self.open_table(tables::LEX_BLOCK)?;
        if let Some(bytes) = bytes {
            ensure!(bytes.len() <= MAX_BLOCK_BYTES, "oversized posting block");
            table.insert(key.as_slice(), bytes)?;
        } else {
            table.remove(key.as_slice())?;
        }
        Ok(())
    }
}

fn varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 128 {
        out.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    out.push(value as u8);
}
fn read_varint(bytes: &mut &[u8]) -> Result<u64> {
    let mut result = 0u64;
    for i in 0..10 {
        let b = *bytes.first().context("truncated posting delta")?;
        *bytes = &bytes[1..];
        ensure!(i != 9 || b <= 1, "posting delta overflow");
        result |= u64::from(b & 127) << (7 * i);
        if b & 128 == 0 {
            ensure!(i == 0 || b != 0, "noncanonical posting delta");
            return Ok(result);
        }
    }
    anyhow::bail!("unterminated posting delta")
}
fn encode(keys: &[u64]) -> Result<Vec<u8>> {
    ensure!(
        !keys.is_empty() && keys.len() <= MAX_BLOCK_KEYS,
        "posting block count bound"
    );
    let mut out = Vec::with_capacity(10 + keys.len() * 2);
    out.extend_from_slice(&(keys.len() as u16).to_be_bytes());
    out.extend_from_slice(&keys[0].to_be_bytes());
    for w in keys.windows(2) {
        ensure!(w[0] < w[1], "unordered posting block");
        varint(w[1] - w[0], &mut out);
    }
    Ok(out)
}
fn decode(mut bytes: &[u8], first: u64, budget: &mut Budget) -> Result<Vec<u64>> {
    ensure!(
        (10..=MAX_BLOCK_BYTES).contains(&bytes.len()),
        "posting block byte bound"
    );
    let n = usize::from(u16::from_be_bytes(bytes[..2].try_into()?));
    ensure!(
        (1..=MAX_BLOCK_KEYS).contains(&n),
        "posting block count bound"
    );
    budget.entries = budget
        .entries
        .checked_sub(n as u64)
        .ok_or(super::directory::Exhausted("decoded posting"))?;
    ensure!(
        u64::from_be_bytes(bytes[2..10].try_into()?) == first,
        "posting block key mismatch"
    );
    bytes = &bytes[10..];
    let mut keys = Vec::with_capacity(n);
    keys.push(first);
    for _ in 1..n {
        let delta = read_varint(&mut bytes)?;
        ensure!(delta > 0, "duplicate posting key");
        keys.push(
            keys.last()
                .unwrap()
                .checked_add(delta)
                .context("posting key overflow")?,
        );
    }
    ensure!(bytes.is_empty(), "trailing posting block bytes");
    Ok(keys)
}
fn key(bytes: &[u8]) -> Result<u64> {
    ensure!(bytes.len() == 8, "invalid posting directory key");
    Ok(u64::from_be_bytes(bytes.try_into()?))
}
pub struct Postings {
    tree: Tree,
    term: Sha256Digest,
}
/// Request-local decoding cache for monotonically increasing intersection
/// candidates. A posting block is charged and decoded once while it is reused.
pub struct Membership {
    postings: Postings,
    header: Header,
    block: Vec<u64>,
    previous: Option<u64>,
}
impl Membership {
    pub fn new(scope: &CurrentScope, term: Sha256Digest, header: Header) -> Result<Self> {
        Ok(Self {
            postings: Postings::new(scope, term)?,
            header,
            block: vec![],
            previous: None,
        })
    }
    pub fn contains(
        &mut self,
        storage: &impl ReadPostings,
        doc: u64,
        budget: &mut Budget,
    ) -> Result<bool> {
        ensure!(
            self.previous.is_none_or(|p| p <= doc),
            "intersection candidates are unordered"
        );
        self.previous = Some(doc);
        if self.block.last().is_none_or(|last| *last < doc) {
            self.block = match self.postings.tree.floor(
                storage,
                self.header.root.as_ref(),
                &doc.to_be_bytes(),
                budget,
            )? {
                Some((k, b)) => self.postings.decode_block(storage, &k, &b, budget)?,
                None => vec![],
            };
        }
        Ok(self.block.binary_search(&doc).is_ok())
    }
}
impl Postings {
    fn block_key(&self, first: &[u8]) -> Result<[u8; 40]> {
        key(first)?;
        let mut result = [0; 40];
        result[..32].copy_from_slice(self.term.as_bytes());
        result[32..].copy_from_slice(first);
        Ok(result)
    }
    fn decode_block(
        &self,
        storage: &impl ReadPostings,
        first: &[u8],
        reference: &[u8],
        budget: &mut Budget,
    ) -> Result<Vec<u64>> {
        ensure!(reference.len() == 44, "invalid posting block reference");
        let first_key = key(first)?;
        let last = u64::from_be_bytes(reference[32..40].try_into()?);
        let count = u32::from_be_bytes(reference[40..44].try_into()?) as usize;
        ensure!(
            first_key <= last && (1..=MAX_BLOCK_KEYS).contains(&count),
            "invalid posting block range/count"
        );
        let encoded = storage.read_block(&self.block_key(first)?, budget)?;
        ensure!(
            self.tree.payload_digest(&encoded).as_bytes().as_slice() == &reference[..32],
            "posting block digest mismatch"
        );
        let keys = decode(&encoded, first_key, budget)?;
        ensure!(
            keys.len() == count && keys.last() == Some(&last),
            "posting block range/count mismatch"
        );
        Ok(keys)
    }
    fn save_block(
        &self,
        storage: &impl WritePostings,
        header: &mut Header,
        keys: &[u64],
        budget: &mut Budget,
    ) -> Result<Vec<u8>> {
        let encoded = encode(keys)?;
        ensure!(
            encoded.len() <= MAX_BLOCK_BYTES,
            "posting block byte bound exceeded"
        );
        let mut reference = self.tree.payload_digest(&encoded).as_bytes().to_vec();
        reference.extend_from_slice(&keys.last().context("empty posting block")?.to_be_bytes());
        reference.extend_from_slice(&(keys.len() as u32).to_be_bytes());
        let first = keys[0].to_be_bytes();
        storage.put_block(&self.block_key(&first)?, Some(&encoded))?;
        header.root = self.tree.put(
            storage,
            header.root.as_ref(),
            &first,
            Some(&reference),
            budget,
        )?;
        Ok(reference)
    }
    fn remove_block(
        &self,
        storage: &impl WritePostings,
        header: &mut Header,
        key: &[u8],
        budget: &mut Budget,
    ) -> Result<()> {
        storage.put_block(&self.block_key(key)?, None)?;
        header.root = self
            .tree
            .put(storage, header.root.as_ref(), key, None, budget)?;
        Ok(())
    }
    pub fn first_at_or_after(
        &self,
        storage: &impl ReadPostings,
        header: &Header,
        start: u64,
        budget: &mut Budget,
    ) -> Result<Option<u64>> {
        if let Some((k, b)) =
            self.tree
                .floor(storage, header.root.as_ref(), &start.to_be_bytes(), budget)?
        {
            let keys = self.decode_block(storage, &k, &b, budget)?;
            if let Some(found) = keys.into_iter().find(|d| *d >= start) {
                return Ok(Some(found));
            }
        }
        self.tree
            .ceiling(storage, header.root.as_ref(), &start.to_be_bytes(), budget)?
            .map(|(k, b)| {
                self.decode_block(storage, &k, &b, budget)
                    .map(|keys| keys[0])
            })
            .transpose()
    }
    /// Replace one path's contiguous doc-key range. Only boundary and affected
    /// blocks are decoded; a common term does not require a corpus-wide scan.
    pub fn replace_range(
        &self,
        storage: &impl WritePostings,
        header: &mut Header,
        start: u64,
        end: u64,
        documents: &[u64],
        budget: &mut Budget,
    ) -> Result<Vec<u64>> {
        ensure!(
            start < end
                && documents.len() <= 8192
                && documents.iter().all(|d| (start..end).contains(d))
                && documents.windows(2).all(|w| w[0] < w[1]),
            "invalid posting replacement range"
        );
        let first = self
            .tree
            .floor(storage, header.root.as_ref(), &start.to_be_bytes(), budget)?
            .map(|(k, _)| k)
            .unwrap_or_else(|| start.to_be_bytes().to_vec());
        let mut old_keys = Vec::new();
        let mut retained = Vec::new();
        let mut removed = Vec::new();
        let mut previous = None;
        self.tree.visit_budgeted(
            storage,
            header.root.as_ref(),
            &first,
            Some(&end.to_be_bytes()),
            budget,
            &mut |k, b, budget| {
                let decoded = self.decode_block(storage, k, b, budget)?;
                ensure!(
                    previous.is_none_or(|p| p < decoded[0]),
                    "overlapping posting blocks"
                );
                previous = decoded.last().copied();
                ensure!(
                    old_keys.len() <= 8192,
                    "posting replacement block bound exceeded"
                );
                old_keys.push(k.to_vec());
                for d in decoded {
                    if (start..end).contains(&d) {
                        ensure!(removed.len() < 8192, "posting removed-key bound exceeded");
                        removed.push(d);
                    } else {
                        ensure!(
                            retained.len() < MAX_BLOCK_KEYS * 2,
                            "posting replacement boundary bound exceeded"
                        );
                        retained.push(d);
                    }
                }
                Ok(())
            },
        )?;
        let mut next = header.clone();
        next.count = next
            .count
            .checked_sub(removed.len() as u64)
            .and_then(|n| n.checked_add(documents.len() as u64))
            .context("posting replacement count overflow")?;
        for k in old_keys {
            self.remove_block(storage, &mut next, &k, budget)?;
        }
        retained.extend_from_slice(documents);
        retained.sort_unstable();
        if !retained.is_empty() {
            self.install(storage, &mut next, &retained, budget)?;
        }
        self.coalesce(storage, &mut next, start, end, budget)?;
        ensure!(
            next.root.is_some() == (next.count > 0),
            "posting replacement root/count mismatch"
        );
        *header = next;
        Ok(removed)
    }
    fn coalesce(
        &self,
        storage: &impl WritePostings,
        header: &mut Header,
        start: u64,
        end: u64,
        budget: &mut Budget,
    ) -> Result<()> {
        let mut left = self.tree.floor(
            storage,
            header.root.as_ref(),
            &start.saturating_sub(1).to_be_bytes(),
            budget,
        )?;
        if left.is_none() {
            left =
                self.tree
                    .ceiling(storage, header.root.as_ref(), &start.to_be_bytes(), budget)?;
        }
        while let Some((left_key, left_bytes)) = left {
            let first = key(&left_key)?;
            if first >= end {
                break;
            }
            let Some(next) = first.checked_add(1) else {
                break;
            };
            let Some((right_key, right_bytes)) =
                self.tree
                    .ceiling(storage, header.root.as_ref(), &next.to_be_bytes(), budget)?
            else {
                break;
            };
            let mut a = self.decode_block(storage, &left_key, &left_bytes, budget)?;
            let b = self.decode_block(storage, &right_key, &right_bytes, budget)?;
            ensure!(
                a.last().unwrap() < &b[0],
                "overlapping neighboring posting blocks"
            );
            let underfilled = a.len() <= MAX_BLOCK_KEYS / 2 || b.len() <= MAX_BLOCK_KEYS / 2;
            if underfilled && a.len() + b.len() <= MAX_BLOCK_KEYS {
                a.extend(b);
                let combined = encode(&a)?;
                if combined.len() <= MAX_BLOCK_BYTES {
                    self.remove_block(storage, header, &right_key, budget)?;
                    let reference = self.save_block(storage, header, &a, budget)?;
                    left = Some((left_key, reference));
                    continue;
                }
            }
            left = Some((right_key, right_bytes));
        }
        Ok(())
    }
    pub fn new(scope: &CurrentScope, term: Sha256Digest) -> Result<Self> {
        let mut domain = b"postings/1/".to_vec();
        domain.extend_from_slice(term.as_bytes());
        Ok(Self {
            tree: Tree::new(scope, &domain)?,
            term,
        })
    }
    pub fn contains(
        &self,
        storage: &impl ReadPostings,
        header: &Header,
        doc: u64,
        budget: &mut Budget,
    ) -> Result<bool> {
        let Some((k, b)) =
            self.tree
                .floor(storage, header.root.as_ref(), &doc.to_be_bytes(), budget)?
        else {
            return Ok(false);
        };
        let keys = self.decode_block(storage, &k, &b, budget)?;
        Ok(keys.binary_search(&doc).is_ok())
    }
    pub fn visit(
        &self,
        storage: &impl ReadPostings,
        header: &Header,
        budget: &mut Budget,
        visitor: &mut impl FnMut(u64) -> Result<()>,
    ) -> Result<()> {
        self.visit_budgeted(storage, header, budget, &mut |doc, _| visitor(doc))
    }
    pub fn visit_budgeted(
        &self,
        storage: &impl ReadPostings,
        header: &Header,
        budget: &mut Budget,
        visitor: &mut impl FnMut(u64, &mut Budget) -> Result<()>,
    ) -> Result<()> {
        let mut seen = 0u64;
        let mut previous = None;
        // The directory shares one budget; decoded keys have their own debit.
        self.tree.visit_budgeted(
            storage,
            header.root.as_ref(),
            &[],
            None,
            budget,
            &mut |k, b, budget| {
                let keys = self.decode_block(storage, k, b, budget)?;
                ensure!(
                    previous.is_none_or(|p| p < keys[0]),
                    "overlapping posting blocks"
                );
                previous = keys.last().copied();
                seen = seen
                    .checked_add(keys.len() as u64)
                    .context("posting count overflow")?;
                ensure!(seen <= header.count, "posting count exceeded");
                for doc in keys {
                    visitor(doc, budget)?;
                }
                Ok(())
            },
        )?;
        ensure!(seen == header.count, "posting count mismatch");
        Ok(())
    }
    pub fn change(
        &self,
        storage: &impl WritePostings,
        header: &mut Header,
        doc: u64,
        present: bool,
        budget: &mut Budget,
    ) -> Result<()> {
        let old = self
            .tree
            .floor(storage, header.root.as_ref(), &doc.to_be_bytes(), budget)?;
        let (old_key, mut keys) = match old {
            Some((k, b)) => (Some(k.clone()), self.decode_block(storage, &k, &b, budget)?),
            None => (None, vec![]),
        };
        match (keys.binary_search(&doc), present) {
            (Ok(_), true) | (Err(_), false) => return Ok(()),
            (Ok(i), false) => {
                keys.remove(i);
                header.count = header
                    .count
                    .checked_sub(1)
                    .context("posting count underflow")?;
            }
            (Err(i), true) => {
                keys.insert(i, doc);
                header.count = header
                    .count
                    .checked_add(1)
                    .context("posting count overflow")?;
            }
        }
        if let Some(k) = old_key {
            self.remove_block(storage, header, &k, budget)?;
        }
        if keys.is_empty() {
            return Ok(());
        }
        self.install(storage, header, &keys, budget)
    }
    fn install(
        &self,
        storage: &impl WritePostings,
        header: &mut Header,
        keys: &[u64],
        budget: &mut Budget,
    ) -> Result<()> {
        let encoded = encode(keys);
        if let Ok(encoded) = encoded
            && encoded.len() <= MAX_BLOCK_BYTES
        {
            self.save_block(storage, header, keys, budget)?;
            return Ok(());
        }
        ensure!(keys.len() > 1, "posting block cannot fit");
        let (left, right) = keys.split_at(keys.len() / 2);
        self.install(storage, header, left, budget)?;
        self.install(storage, header, right, budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::identity::Incarnation;
    #[test]
    fn delta_codec_rejects_noncanonical_overflow_and_trailing_bytes() -> Result<()> {
        for keys in [vec![0], vec![0, 1, 127, 128, u64::MAX], vec![u64::MAX]] {
            let bytes = encode(&keys)?;
            assert_eq!(decode(&bytes, keys[0], &mut Budget::query())?, keys);
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(decode(&trailing, keys[0], &mut Budget::query()).is_err());
        }
        let mut duplicate = encode(&[0, 1])?;
        duplicate[10] = 0;
        assert!(decode(&duplicate, 0, &mut Budget::query()).is_err());
        let mut noncanonical = encode(&[0, 1])?;
        noncanonical[10] = 129;
        noncanonical.push(0);
        assert!(decode(&noncanonical, 0, &mut Budget::query()).is_err());
        let mut overflow = encode(&[u64::MAX - 1, u64::MAX])?;
        overflow[10] = 2;
        assert!(decode(&overflow, u64::MAX - 1, &mut Budget::query()).is_err());
        Ok(())
    }
    #[test]
    fn path_range_updates_split_blocks_preserve_neighbors_and_charge_intersection_once()
    -> Result<()> {
        let scope = CurrentScope {
            root: "fixture/posting".into(),
            collection: "posting".into(),
            incarnation: Incarnation::from_bytes([8; 16]),
        };
        let term = Sha256Digest::from_bytes([9; 32]);
        let postings = Postings::new(&scope, term)?;
        let file = tempfile::NamedTempFile::new()?;
        let db = super::super::Database::open(file.reopen()?, 32 << 20, true)?;
        let mut budget = Budget {
            pages: 1_000_000,
            bytes: 16 << 30,
            entries: 1_000_000,
        };
        let mut header = Header::default();
        let tx = db.write()?;
        let first: Vec<_> = (0..7000u64).collect();
        let second: Vec<_> = (0..7000u64).map(|i| (1 << 16) | i).collect();
        postings.replace_range(&tx, &mut header, 0, 1 << 16, &first, &mut budget)?;
        postings.replace_range(&tx, &mut header, 1 << 16, 2 << 16, &second, &mut budget)?;
        assert_eq!(header.count, 14_000);
        tx.commit()?;
        let old = db.snapshot()?;
        let old_header = header.clone();
        let tx = db.write()?;
        assert_eq!(
            postings
                .replace_range(&tx, &mut header, 0, 1 << 16, &[2, 9, 6999], &mut budget)?
                .len(),
            7000
        );
        tx.commit()?;
        let mut values = Vec::new();
        postings.visit(
            db.snapshot()?.transaction(),
            &header,
            &mut budget,
            &mut |d| {
                values.push(d);
                Ok(())
            },
        )?;
        let mut expected = vec![2, 9, 6999];
        expected.extend(second.clone());
        assert_eq!(values, expected);
        let mut member = Membership::new(&scope, term, old_header.clone())?;
        let mut query_budget = Budget::query();
        for doc in first {
            assert!(member.contains(old.transaction(), doc, &mut query_budget)?);
        }
        assert!(
            query_budget.entries > 1_000_000,
            "intersection repeatedly decoded its current block"
        );
        assert_eq!(old_header.count, 14_000);
        Ok(())
    }
}
