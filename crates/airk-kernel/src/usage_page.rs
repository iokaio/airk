// SPDX-License-Identifier: Apache-2.0
//! Exact content-free report pages fit the direct worker's existing output credit.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_REPORT_BYTES: usize = 4 << 20;
const MAX_RECORD_BYTES: usize = 16 << 10;
const MAX_RECORDS: usize = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Key(String),
    Index(usize),
    StringChunk { offset: usize },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub path: Vec<Part>,
    pub value: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub digest: String,
    pub items: Vec<Record>,
    pub cursor: Option<String>,
    pub record_bytes: usize,
    pub total_items: usize,
}

fn split(value: Value, path: &mut Vec<Part>, records: &mut Vec<Record>) -> Result<()> {
    ensure!(
        path.len() <= 32 && records.len() < MAX_RECORDS,
        "usage report topology exceeds budget"
    );
    let record = Record {
        path: path.clone(),
        value,
    };
    if serde_json::to_vec(&record)?.len() <= MAX_RECORD_BYTES {
        records.push(record);
        return Ok(());
    }
    match record.value {
        Value::String(value) => {
            let mut offset = 0;
            let mut characters = value.chars().peekable();
            while characters.peek().is_some() {
                let chunk = characters.by_ref().take(2000).collect::<String>();
                path.push(Part::StringChunk { offset });
                let record = Record {
                    path: path.clone(),
                    value: Value::String(chunk.clone()),
                };
                ensure!(
                    records.len() < MAX_RECORDS
                        && serde_json::to_vec(&record)?.len() <= MAX_RECORD_BYTES,
                    "usage string chunk exceeds record budget"
                );
                records.push(record);
                path.pop();
                offset += chunk.len();
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                path.push(Part::Key(key));
                split(value, path, records)?;
                path.pop();
            }
        }
        Value::Array(values) => {
            for (index, value) in values.into_iter().enumerate() {
                path.push(Part::Index(index));
                split(value, path, records)?;
                path.pop();
            }
        }
        _ => anyhow::bail!("usage report scalar exceeds record budget"),
    }
    Ok(())
}

pub fn page(value: Value, cursor: Option<&str>, max_bytes: usize) -> Result<Page> {
    ensure!(
        (20 << 10..=48 << 10).contains(&max_bytes),
        "usage page budget must be 20..48 KiB"
    );
    let bytes = serde_json::to_vec(&value)?;
    ensure!(
        bytes.len() <= MAX_REPORT_BYTES,
        "usage report exceeds bounded snapshot budget"
    );
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let offset = match cursor {
        None => 0,
        Some(cursor) => {
            ensure!(cursor.len() <= 100, "invalid usage cursor");
            let mut fields = cursor.split(':');
            ensure!(
                fields.next() == Some("airk-u1") && fields.next() == Some(digest.as_str()),
                "usage report changed; restart report pagination"
            );
            let offset: usize = fields
                .next()
                .ok_or_else(|| anyhow::anyhow!("invalid usage cursor"))?
                .parse()?;
            ensure!(
                fields.next().is_none() && offset > 0,
                "invalid usage cursor"
            );
            offset
        }
    };
    let mut records = Vec::new();
    split(value, &mut Vec::new(), &mut records)?;
    ensure!(offset < records.len(), "usage cursor exceeds snapshot");
    let total_items = records.len();
    let mut result = Page {
        digest,
        items: Vec::new(),
        cursor: None,
        record_bytes: 0,
        total_items,
    };
    for record in records.into_iter().skip(offset) {
        let size = serde_json::to_vec(&record)?.len();
        if result.record_bytes + size > max_bytes - 1024 {
            break;
        }
        result.record_bytes += size;
        result.items.push(record);
    }
    ensure!(!result.items.is_empty(), "usage page cannot fit one record");
    let next = offset + result.items.len();
    if next < total_items {
        result.cursor = Some(format!("airk-u1:{}:{next}", result.digest));
    }
    ensure!(
        serde_json::to_vec(&result)?.len() <= max_bytes,
        "usage page framing exceeds budget"
    );
    Ok(result)
}

pub fn assemble(document: &mut Value, record: Record) -> Result<()> {
    let Record { mut path, value } = record;
    let string_offset = if let Some(Part::StringChunk { offset }) = path.last() {
        let offset = *offset;
        path.pop();
        Some(offset)
    } else {
        None
    };
    let mut target = document;
    for part in path {
        target = match part {
            Part::Key(key) => {
                if target.is_null() {
                    *target = serde_json::json!({});
                }
                target
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("crossed usage object path"))?
                    .entry(key)
                    .or_insert(Value::Null)
            }
            Part::Index(index) => {
                ensure!(index < MAX_RECORDS, "usage array index exceeds budget");
                if target.is_null() {
                    *target = serde_json::json!([]);
                }
                let values = target
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("crossed usage array path"))?;
                values.resize(values.len().max(index + 1), Value::Null);
                &mut values[index]
            }
            Part::StringChunk { .. } => anyhow::bail!("usage string chunk must end its path"),
        };
    }
    if let Some(offset) = string_offset {
        ensure!(
            offset <= MAX_REPORT_BYTES,
            "usage string offset exceeds budget"
        );
        if target.is_null() {
            *target = Value::String(String::new());
        }
        let Value::String(text) = target else {
            anyhow::bail!("crossed usage string path")
        };
        ensure!(
            text.len() == offset,
            "usage string chunks overlap or are missing"
        );
        let chunk = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("invalid usage string chunk"))?;
        ensure!(
            text.len() + chunk.len() <= MAX_REPORT_BYTES,
            "usage string exceeds budget"
        );
        text.push_str(chunk);
        return Ok(());
    }
    ensure!(target.is_null(), "overlapping usage page records");
    *target = value;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_pages_preserve_arrays_empty_objects_and_reject_changed_snapshots() -> Result<()> {
        let original = serde_json::json!({"history":(0..30).map(|day|serde_json::json!({"day":day,"counts":vec![day;3000]})).collect::<Vec<_>>(),"empty":{},"null":null,"export":"\"月\",report\n".repeat(6000)});
        let mut cursor = None;
        let mut restored = Value::Null;
        let mut pages = 0;
        loop {
            let next = page(original.clone(), cursor.as_deref(), 24 << 10)?;
            assert!(serde_json::to_vec(&next)?.len() <= 24 << 10);
            if let Some(cursor) = &next.cursor {
                let mut changed = original.clone();
                changed["null"] = serde_json::json!(1);
                assert!(page(changed, Some(cursor), 24 << 10).is_err());
            }
            for record in next.items {
                assemble(&mut restored, record)?;
            }
            pages += 1;
            cursor = next.cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert!(pages > 1);
        assert_eq!(restored, original);
        assert!(page(original, Some("airk-u1:bad:1"), 24 << 10).is_err());
        Ok(())
    }
}
