// SPDX-License-Identifier: Apache-2.0
//! Bounded ONNX external-data admission before native graph parsing.
//! Field numbers come from pinned ONNX 1.22 onnx.proto. No tensor bytes are
//! interpreted. Every external location is supplied from verified memory.
use anyhow::{Context, Result, bail, ensure};
use std::collections::{BTreeMap, BTreeSet};

const MAX_GRAPH: usize = 64 << 20;
const MAX_FIELDS: usize = 2_000_000;

enum Field<'a> {
    Integer(u64),
    Bytes(&'a [u8]),
    Fixed,
}
struct Cursor<'a>(&'a [u8]);
impl<'a> Cursor<'a> {
    fn integer(&mut self) -> Result<u64> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let (&byte, rest) = self.0.split_first().context("truncated ONNX integer")?;
            self.0 = rest;
            ensure!(shift != 63 || byte <= 1, "overflowing ONNX integer");
            value |= u64::from(byte & 127) << shift;
            if byte < 128 {
                return Ok(value);
            }
        }
        bail!("overflowing ONNX integer")
    }
    fn take(&mut self, size: usize) -> Result<&'a [u8]> {
        ensure!(size <= self.0.len(), "truncated ONNX field");
        let (bytes, rest) = self.0.split_at(size);
        self.0 = rest;
        Ok(bytes)
    }
    fn field(&mut self) -> Result<Option<(u64, Field<'a>)>> {
        if self.0.is_empty() {
            return Ok(None);
        }
        let tag = self.integer()?;
        let number = tag >> 3;
        ensure!((1..(1 << 29)).contains(&number), "invalid ONNX field tag");
        let value = match tag & 7 {
            0 => Field::Integer(self.integer()?),
            1 => {
                self.take(8)?;
                Field::Fixed
            }
            2 => {
                let size = usize::try_from(self.integer()?)?;
                Field::Bytes(self.take(size)?)
            }
            5 => {
                self.take(4)?;
                Field::Fixed
            }
            _ => bail!("unsupported ONNX protobuf group"),
        };
        Ok(Some((number, value)))
    }
}

#[derive(Clone, Copy)]
enum Message {
    Model,
    Graph,
    Node,
    Attribute,
    Tensor,
    Sparse,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LargeInitializer {
    pub file: String,
    pub name: String,
    pub offset: u64,
    pub length: u64,
    pub shape: Vec<i64>,
    pub element: u64,
}

struct Audit<'a> {
    files: &'a BTreeMap<String, u64>,
    external: BTreeSet<String>,
    fields: usize,
    large: Vec<LargeInitializer>,
}
impl Audit<'_> {
    fn visit(&mut self, bytes: &[u8], message: Message, depth: usize) -> Result<()> {
        ensure!(depth <= 64, "ONNX graph nesting limit");
        let mut cursor = Cursor(bytes);
        let mut external = BTreeMap::new();
        let mut location = None;
        let mut raw = false;
        let mut tensor_name = None;
        let mut tensor_element = None;
        let mut shape = Vec::new();
        while let Some((number, field)) = cursor.field()? {
            self.fields += 1;
            ensure!(self.fields <= MAX_FIELDS, "ONNX graph field limit");
            match (message, number, field) {
                (Message::Tensor, 1, Field::Integer(value)) => shape.push(i64::try_from(value)?),
                (Message::Tensor, 1, Field::Bytes(bytes)) => {
                    let mut packed = Cursor(bytes);
                    while !packed.0.is_empty() {
                        shape.push(i64::try_from(packed.integer()?)?);
                    }
                }
                (Message::Tensor, 2, Field::Integer(value)) => {
                    ensure!(
                        tensor_element.replace(value).is_none(),
                        "duplicate ONNX tensor element"
                    );
                }
                (Message::Tensor, 8, Field::Bytes(bytes)) => {
                    ensure!(
                        tensor_name.replace(std::str::from_utf8(bytes)?).is_none(),
                        "duplicate ONNX tensor name"
                    );
                }
                (Message::Model, 7, Field::Bytes(bytes)) => {
                    self.visit(bytes, Message::Graph, depth + 1)?
                }
                // Product inference exports do not carry training graphs or
                // model-local functions with additional unreviewed graph roots.
                (Message::Model, 20 | 25, _) => {
                    bail!("unsupported ONNX training or local function graph")
                }
                (Message::Graph, 1, Field::Bytes(bytes)) => {
                    self.visit(bytes, Message::Node, depth + 1)?
                }
                (Message::Graph, 5, Field::Bytes(bytes)) => {
                    self.visit(bytes, Message::Tensor, depth + 1)?
                }
                (Message::Graph, 15, Field::Bytes(bytes)) => {
                    self.visit(bytes, Message::Sparse, depth + 1)?
                }
                (Message::Node, 5, Field::Bytes(bytes)) => {
                    self.visit(bytes, Message::Attribute, depth + 1)?
                }
                (Message::Attribute, 5 | 10, Field::Bytes(bytes)) => {
                    self.visit(bytes, Message::Tensor, depth + 1)?
                }
                (Message::Attribute, 6 | 11, Field::Bytes(bytes)) => {
                    self.visit(bytes, Message::Graph, depth + 1)?
                }
                (Message::Attribute, 22 | 23, Field::Bytes(bytes)) => {
                    self.visit(bytes, Message::Sparse, depth + 1)?
                }
                (Message::Sparse, 1 | 2, Field::Bytes(bytes)) => {
                    self.visit(bytes, Message::Tensor, depth + 1)?
                }
                (Message::Tensor, 13, Field::Bytes(bytes)) => {
                    let mut entry = Cursor(bytes);
                    let mut key = None;
                    let mut value = None;
                    while let Some((number, field)) = entry.field()? {
                        match (number, field) {
                            (1, Field::Bytes(bytes)) if key.is_none() => {
                                key = Some(std::str::from_utf8(bytes)?)
                            }
                            (2, Field::Bytes(bytes)) if value.is_none() => {
                                value = Some(std::str::from_utf8(bytes)?)
                            }
                            _ => bail!("invalid ONNX external-data map entry"),
                        }
                    }
                    ensure!(
                        external.len() < 4
                            && external
                                .insert(
                                    key.context("missing ONNX external key")?,
                                    value.context("missing ONNX external value")?
                                )
                                .is_none(),
                        "duplicate ONNX external-data key"
                    );
                }
                (Message::Tensor, 14, Field::Integer(value)) => {
                    ensure!(
                        location.replace(value).is_none(),
                        "duplicate ONNX data location"
                    );
                }
                (Message::Tensor, 9, _) => raw = true,
                _ => {}
            }
        }
        if matches!(message, Message::Tensor) {
            if external.is_empty() {
                ensure!(
                    matches!(location, None | Some(0)),
                    "ONNX external tensor has no location"
                );
                return Ok(());
            }
            ensure!(
                depth == 2,
                "external ONNX data is supported only in main graph direct initializers"
            );
            ensure!(location == Some(1) && !raw, "ambiguous ONNX tensor data");
            ensure!(
                external
                    .keys()
                    .all(|key| matches!(*key, "location" | "offset" | "length")),
                "unsupported ONNX external-data property"
            );
            let name = *external
                .get("location")
                .context("ONNX external filename missing")?;
            ensure!(
                !name.is_empty()
                    && name.len() <= 128
                    && name != "."
                    && name != ".."
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric()
                            || matches!(byte, b'.' | b'-' | b'_')),
                "unsafe ONNX external filename"
            );
            let size = *self
                .files
                .get(name)
                .context("unverified ONNX external file")?;
            let decimal = |value: &&str| -> Result<u64> {
                ensure!(
                    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()),
                    "invalid ONNX external range"
                );
                Ok(value.parse()?)
            };
            let offset = external
                .get("offset")
                .map(decimal)
                .transpose()?
                .unwrap_or(0);
            let length = external
                .get("length")
                .map(decimal)
                .transpose()?
                .unwrap_or(size.saturating_sub(offset));
            ensure!(
                length > 0 && offset.checked_add(length).is_some_and(|end| end <= size),
                "ONNX external range exceeds verified data"
            );
            if length >= (1u64 << 31) {
                // ORT 1.30's file-in-memory API embeds raw TensorProto bytes,
                // whose per-tensor size is bounded below this boundary. Its
                // OrtValue API retains large tensors as external memory instead.
                ensure!(depth == 2, "large nested ONNX initializer unsupported");
                let element = tensor_element.context("large tensor element missing")?;
                let width = match element {
                    1 => 4u64,
                    2 => 1,
                    _ => bail!("unsupported large external tensor element"),
                };
                ensure!(
                    shape.len() <= 16 && shape.iter().all(|axis| *axis > 0),
                    "invalid large external shape"
                );
                let expected = shape
                    .iter()
                    .try_fold(width, |count, axis| count.checked_mul(*axis as u64))
                    .context("large external shape overflow")?;
                ensure!(
                    expected == length,
                    "large external tensor byte count differs from shape"
                );
                let tensor_name = tensor_name.context("large external tensor name missing")?;
                ensure!(
                    !tensor_name.is_empty()
                        && tensor_name.len() <= 1024
                        && !tensor_name.contains('\0')
                        && !self.large.iter().any(|item| item.name == tensor_name),
                    "unsafe or duplicated large initializer name"
                );
                self.large.push(LargeInitializer {
                    file: name.into(),
                    name: tensor_name.into(),
                    offset,
                    length,
                    shape,
                    element,
                });
            }
            self.external.insert(name.to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

pub(crate) fn validate(model: &[u8], files: &BTreeMap<String, u64>) -> Result<BTreeSet<String>> {
    ensure!(
        !model.is_empty() && model.len() <= MAX_GRAPH && files.len() <= 128,
        "ONNX graph or external catalog admission limit"
    );
    let mut audit = Audit {
        files,
        external: BTreeSet::new(),
        fields: 0,
        large: Vec::new(),
    };
    audit.visit(model, Message::Model, 0)?;
    ensure!(
        audit.external.len() == files.len(),
        "unused ONNX external asset"
    );
    Ok(audit.external)
}

/// Metadata-only plan; no allocation proportional to tensor length.
pub(crate) fn large_initializers(
    model: &[u8],
    files: &BTreeMap<String, u64>,
) -> Result<Vec<LargeInitializer>> {
    ensure!(
        !model.is_empty() && model.len() <= MAX_GRAPH && files.len() <= 128,
        "ONNX graph admission limit"
    );
    let mut audit = Audit {
        files,
        external: BTreeSet::new(),
        fields: 0,
        large: Vec::new(),
    };
    audit.visit(model, Message::Model, 0)?;
    ensure!(
        audit.external.len() == files.len(),
        "unused ONNX external asset"
    );
    Ok(audit.large)
}
