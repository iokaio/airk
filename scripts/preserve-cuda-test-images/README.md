# CUDA native image preservation

`scripts/preserve-cuda-test-images.py` authenticates actual successful CargoJSON
build results and copies native test images into fresh reviewed locations. It
never executes an image or model. CONTRACT.md defines custody, source, runtime
census, static-link and same-human ownership requirements.

Use `python -B scripts/preserve-cuda-test-images.py --help` for required reviewed
receipt/log/source/census pins. The legacy mode retains its fixed historical
archive, environment and native manifest. Explicit incremental-finalgraph mode
uses the dedicated new environment/producer proof chain, actual Cargo static
archive path, native manifest producer identity and actual PE collector exports.
Reviewed source hashes supplement verification; they never replace producer proof.

Preservation is create-only and additive. Existing runtime objects retain exact
bytes, file identities and access policy; new images are independent copies.
Packets and binding receipts support later plan validation, not native execution
or qualification claims. Failures remain evidence and do not authorize overwrite,
cleanup, retries or a different account.

Run pure controls with `python -B scripts/tests/test_preserve_cuda_test_images.py`.
Before actual preservation, enter the verified human shell and follow all output
parent/default-owner and postcreation audits. No source or model fixtures are
copied into maintained code or distributed packages by this helper.
