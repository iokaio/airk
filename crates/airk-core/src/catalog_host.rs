// SPDX-License-Identifier: Apache-2.0
//! Bounded administrative context identity, separate from evidence permission.
use crate::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::{CatalogRevision, ContextChoices, encoded_size},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Native registry/OS-user identity and an explicitly supplied context key.
/// This is neither a bearer credential nor proof of permission to read evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeHostContextDescriptor {
    pub schema: u32,
    pub namespace: Incarnation,
    pub host: Sha256Digest,
    pub key: Sha256Digest,
    pub catalog_revision: CatalogRevision,
    pub context: Option<ContextChoices>,
}

impl NativeHostContextDescriptor {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported native context descriptor");
        if let Some(context) = &self.context {
            context.validate()?;
            ensure!(
                context.host == self.host
                    && context.key == self.key
                    && context.revision <= self.catalog_revision,
                "native context descriptor association changed"
            );
        }
        encoded_size(self, 16 * 1024)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index_catalog::{ContextId, ContextKind};

    fn descriptor() -> NativeHostContextDescriptor {
        let host = Sha256Digest::from_bytes([2; 32]);
        let key = Sha256Digest::from_bytes([3; 32]);
        NativeHostContextDescriptor {
            schema: 1,
            namespace: Incarnation::from_bytes([1; 16]),
            host,
            key,
            catalog_revision: CatalogRevision::INITIAL,
            context: Some(ContextChoices {
                id: ContextId::from_identity(Incarnation::from_bytes([4; 16])),
                host,
                key,
                kind: ContextKind::Empty,
                name: "Explicit empty context".into(),
                revision: CatalogRevision::INITIAL,
                project_ids: vec![],
                index_ids: vec![],
            }),
        }
    }

    #[test]
    fn context_descriptor_preserves_empty_choices_and_exact_wire_data() -> Result<()> {
        let value = descriptor();
        value.validate()?;
        let wire = serde_json::to_value(&value)?;
        assert!(wire.get("caller").is_none() && wire.get("token").is_none());
        assert_eq!(
            serde_json::from_value::<NativeHostContextDescriptor>(wire)?,
            value
        );
        let mut absent = value;
        absent.context = None;
        absent.validate()?;
        Ok(())
    }

    #[test]
    fn context_descriptor_refuses_foreign_or_future_associations() -> Result<()> {
        let original = descriptor();
        for changed in 0..4 {
            let mut value = original.clone();
            match changed {
                0 => value.schema = 2,
                1 => value.context.as_mut().unwrap().host = Sha256Digest::from_bytes([8; 32]),
                2 => value.context.as_mut().unwrap().key = Sha256Digest::from_bytes([8; 32]),
                _ => value.context.as_mut().unwrap().revision = CatalogRevision::INITIAL.next()?,
            }
            assert!(value.validate().is_err());
        }
        Ok(())
    }
}
