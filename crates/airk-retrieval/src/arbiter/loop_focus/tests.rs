// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn ranking_orders_the_scored_prefix_by_relevance_and_keeps_the_tail() -> Result<()> {
    let ranked = rank(&[(0, 0.2), (1, 0.9), (2, 0.9), (3, 0.1)], 6)?;
    assert_eq!(ranked, vec![1, 2, 0, 3, 4, 5]);
    assert_eq!(rank(&[], 3)?, vec![0, 1, 2]);
    assert!(rank(&[(1, 0.5)], 3).is_err(), "only the prefix is scored");
    assert!(rank(&[(0, f32::NAN)], 3).is_err());
    assert!(rank(&[(0, 0.1), (1, 0.2)], 1).is_err());
    Ok(())
}

#[test]
fn the_batch_identity_binds_the_pair_policy_and_differs_from_it() -> Result<()> {
    let one = Sha256Digest::from_bytes([1; 32]);
    let two = Sha256Digest::from_bytes([2; 32]);
    assert_ne!(policy_digest(one)?, one);
    assert_ne!(policy_digest(one)?, policy_digest(two)?);
    assert_eq!(policy_digest(one)?, policy_digest(one)?);
    Ok(())
}
