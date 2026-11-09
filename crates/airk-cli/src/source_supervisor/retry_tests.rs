// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn unavailable_watch_configuration_retries_only_current_epoch_and_preserves_siblings() {
    let project = Sha256Digest::from_bytes([1; 32]);
    let sibling = Sha256Digest::from_bytes([2; 32]);
    let digest = Sha256Digest::from_bytes([3; 32]);
    let mut sent = BTreeMap::from([(project, (2, Some(digest))), (sibling, (1, Some(digest)))]);
    for failure in [WatchFailure::MetadataUnavailable, WatchFailure::RootLimit] {
        retry_unavailable_configuration(&mut sent, project, 1, Some(failure));
        assert_eq!(sent.get(&project), Some(&(2, Some(digest))));
        retry_unavailable_configuration(&mut sent, project, 2, Some(failure));
        assert!(!sent.contains_key(&project));
        assert_eq!(sent.get(&sibling), Some(&(1, Some(digest))));
        sent.insert(project, (2, Some(digest)));
    }
    for failure in [None, Some(WatchFailure::BackendUnavailable)] {
        retry_unavailable_configuration(&mut sent, project, 2, failure);
        assert_eq!(sent.get(&project), Some(&(2, Some(digest))));
    }
}
