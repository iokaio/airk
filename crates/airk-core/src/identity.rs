// SPDX-License-Identifier: Apache-2.0
//! Version-one identity primitives. Allocation and ancestry verification belong to the kernel.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt, str::FromStr};

pub const IDENTITY_CONTRACT_VERSION: u32 = 1;

macro_rules! hex_identity {
    ($name:ident, $bytes:literal) => {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name([u8; $bytes]);

        impl $name {
            pub const fn from_bytes(bytes: [u8; $bytes]) -> Self {
                Self(bytes)
            }
            pub const fn as_bytes(&self) -> &[u8; $bytes] {
                &self.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in self.0 {
                    write!(f, "{byte:02x}")?;
                }
                Ok(())
            }
        }
        impl FromStr for $name {
            type Err = anyhow::Error;
            fn from_str(value: &str) -> Result<Self> {
                ensure!(
                    value.len() == $bytes * 2
                        && value
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                    "invalid canonical {}",
                    stringify!($name)
                );
                let mut bytes = [0; $bytes];
                for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    let digit = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
                    bytes[index] = digit(pair[0]) * 16 + digit(pair[1]);
                }
                Ok(Self(bytes))
            }
        }
        impl TryFrom<String> for $name {
            type Error = anyhow::Error;
            fn try_from(value: String) -> Result<Self> {
                value.parse()
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.to_string()
            }
        }
    };
}

hex_identity!(Incarnation, 16);
hex_identity!(Sha256Digest, 32);
hex_identity!(ArtifactId, 16);
hex_identity!(SymbolKey, 16);
hex_identity!(DocumentKey, 16);

/// Exact publication selector. A syntactically valid token is not an ancestry proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RevisionToken {
    pub incarnation: Incarnation,
    pub generation: u64,
    pub revision: Sha256Digest,
}

impl fmt::Display for RevisionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "airk-r1:{}:{}:{}",
            self.incarnation, self.generation, self.revision
        )
    }
}

impl FromStr for RevisionToken {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        ensure!(value.len() <= 126, "revision token exceeds bound");
        let fields: Vec<_> = value.split(':').collect();
        ensure!(
            fields.len() == 4 && fields[0] == "airk-r1",
            "unsupported revision token"
        );
        let generation = fields[2];
        ensure!(
            !generation.is_empty()
                && generation.bytes().all(|b| b.is_ascii_digit())
                && (generation == "0" || !generation.starts_with('0')),
            "noncanonical generation"
        );
        Ok(Self {
            incarnation: fields[1].parse()?,
            generation: generation.parse()?,
            revision: fields[3].parse()?,
        })
    }
}

impl TryFrom<String> for RevisionToken {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}

impl From<RevisionToken> for String {
    fn from(value: RevisionToken) -> Self {
        value.to_string()
    }
}

impl RevisionToken {
    /// Checks only scope/order. Earlier revisions still require a stored ancestry proof.
    pub fn validate_against(&self, target: &Self) -> Result<()> {
        ensure!(
            self.incarnation == target.incarnation,
            "foreign collection incarnation"
        );
        ensure!(
            self.generation <= target.generation,
            "future revision token"
        );
        if self.generation == target.generation && self.revision != target.revision {
            bail!("forked revision token");
        }
        Ok(())
    }

    pub fn next_generation(&self) -> Result<u64> {
        self.generation
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("generation overflow"))
    }
}

/// SHA-256 over a fixed magic, domain and fields, each framed with a u64 BE length.
/// Callers supply canonical bytes; this function never serializes legacy snapshots.
pub fn framed_digest(domain: &[u8], fields: &[&[u8]]) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(b"AIRK-DIGEST-1\0");
    for field in std::iter::once(domain).chain(fields.iter().copied()) {
        hash.update((field.len() as u64).to_be_bytes());
        hash.update(field);
    }
    Sha256Digest::from_bytes(hash.finalize().into())
}

/// Inventory excludes its own revision and final manifest/catalog checksums.
pub fn revision_digest(retrieval_specification: &[u8], canonical_inventory: &[u8]) -> Sha256Digest {
    framed_digest(
        b"publication-revision/1",
        &[retrieval_specification, canonical_inventory],
    )
}

/// Trusted only when read from the committed root transaction or validated maintenance state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationReceipt {
    pub token: RevisionToken,
    pub inventory_digest: Sha256Digest,
    pub manifest_digest: Sha256Digest,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(generation: u64) -> RevisionToken {
        RevisionToken {
            incarnation: Incarnation::from_bytes([1; 16]),
            generation,
            revision: Sha256Digest::from_bytes([2; 32]),
        }
    }

    #[test]
    fn exact_tokens_reject_noncanonical_and_truncated_inputs() {
        let value = token(u64::MAX);
        assert_eq!(value.to_string().parse::<RevisionToken>().unwrap(), value);
        assert_eq!(
            serde_json::from_str::<RevisionToken>(&serde_json::to_string(&value).unwrap()).unwrap(),
            value
        );
        for invalid in [
            value.to_string().replace("airk-r1", "airk-r2"),
            token(1).to_string().replace(":1:", ":01:"),
            token(1).to_string().replace(":1:", ":+1:"),
            format!("{}x", value),
            "airk-r1:01:1:02".into(),
        ] {
            assert!(invalid.parse::<RevisionToken>().is_err(), "{invalid}");
        }
        assert!("AA".repeat(16).parse::<Incarnation>().is_err());
        assert!(serde_json::from_str::<Sha256Digest>("\"abc\"").is_err());
    }

    #[test]
    fn scope_order_and_overflow_checks_do_not_claim_ancestry() {
        assert!(token(1).validate_against(&token(2)).is_ok());
        assert!(token(2).validate_against(&token(1)).is_err());
        let mut other = token(1);
        other.revision = Sha256Digest::from_bytes([3; 32]);
        assert!(other.validate_against(&token(1)).is_err());
        other.incarnation = Incarnation::from_bytes([4; 16]);
        assert!(other.validate_against(&token(2)).is_err());
        assert!(token(u64::MAX).next_generation().is_err());
    }

    #[test]
    fn framing_distinguishes_boundaries_and_domains() {
        // Independently calculated with .NET SHA256 using the documented framing.
        assert_eq!(
            revision_digest(b"engine1", b"{}").to_string(),
            "7dd5054a34b9170137a2d4c629737613f1d01f955f527304ee5d1e93cc1b2df9"
        );
        assert_ne!(
            framed_digest(b"x", &[b"ab", b"c"]),
            framed_digest(b"x", &[b"a", b"bc"])
        );
        assert_ne!(framed_digest(b"x", &[b"a"]), framed_digest(b"y", &[b"a"]));
        assert_ne!(framed_digest(b"x", &[]), framed_digest(b"x", &[b""]));
        assert_ne!(
            revision_digest(b"engine1", b"{}"),
            revision_digest(b"engine2", b"{}")
        );
    }
}
