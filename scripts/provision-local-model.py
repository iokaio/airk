# SPDX-License-Identifier: Apache-2.0
"""Deliberately provision pinned Apache-2.0 MiniLM assets; AIRK runtime never downloads."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile
import urllib.request

REVISION = "10dbd2f06a8baf40ad285b037edda610c1b9a57c"
SOURCE = "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2"
ASSETS = {
    "model.onnx": (f"{SOURCE}/resolve/{REVISION}/onnx/model.onnx", 90_405_214,
                   "6fd5d72fe4589f189f8ebc006442dbb529bb7ce38f8082112682524616046452"),
    "tokenizer.json": (f"{SOURCE}/resolve/{REVISION}/tokenizer.json", 466_247,
                       "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037"),
    "LICENSE": ("https://www.apache.org/licenses/LICENSE-2.0.txt", 11_358,
                "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"),
}


def verify(path, expected_size, expected_digest):
    if path.is_symlink() or not path.is_file() or path.stat().st_size != expected_size:
        raise ValueError(f"Invalid local asset size/type: {path.name}")
    digest = hashlib.sha256()
    count = 0
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            count += len(block)
            if count > expected_size:
                raise ValueError(f"Asset grew while verifying: {path.name}")
            digest.update(block)
    if count != expected_size or digest.hexdigest() != expected_digest:
        raise ValueError(f"Asset digest mismatch: {path.name}")


def provision(destination, download=False):
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    if destination.is_symlink():
        raise ValueError("Destination must be an unlinked directory")
    for name, (url, size, digest) in ASSETS.items():
        path = destination / name
        if path.exists() or path.is_symlink():
            verify(path, size, digest)
            continue
        if not download:
            raise ValueError(f"Missing {name}; use --download to explicitly fetch pinned public assets")
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=destination, prefix="model-download-", delete=False) as output:
                temporary = Path(output.name)
                with urllib.request.urlopen(url, timeout=120) as source:
                    count = 0
                    for block in iter(lambda: source.read(1024 * 1024), b""):
                        count += len(block)
                        if count > size:
                            raise ValueError(f"Download exceeds pinned size: {name}")
                        output.write(block)
                output.flush()
                os.fsync(output.fileno())
            verify(temporary, size, digest)
            # Refuse to overwrite assets created by another provisioner.
            with path.open("xb") as output, temporary.open("rb") as source:
                for block in iter(lambda: source.read(1024 * 1024), b""):
                    output.write(block)
                output.flush()
                os.fsync(output.fileno())
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
    manifest = {
        "schema": 2, "name": "sentence-transformers/all-MiniLM-L6-v2",
        "license": "Apache-2.0", "source": SOURCE, "revision": REVISION,
        "model_sha256": ASSETS["model.onnx"][2],
        "tokenizer_sha256": ASSETS["tokenizer.json"][2],
        "license_sha256": ASSETS["LICENSE"][2],
        "dimensions": 384, "max_tokens": 256,
        "pooling": "attention-mask-mean-l2", "input_ids": "input_ids",
        "attention_mask": "attention_mask", "token_type_ids": "token_type_ids",
        "output": "last_hidden_state",
    }
    path = destination / "model.json"
    payload = (json.dumps(manifest, indent=2) + "\n").encode()
    if path.exists() or path.is_symlink():
        if path.is_symlink() or path.read_bytes() != payload:
            raise ValueError("Existing model.json differs; select a new destination")
    else:
        with path.open("xb") as output:
            output.write(payload)
            output.flush()
            os.fsync(output.fileno())
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--download", action="store_true", help="explicitly fetch the pinned public model/tokenizer/license")
    args = parser.parse_args()
    manifest = provision(args.destination, args.download)
    print(json.dumps({"verified": True, "directory": str(args.destination.resolve()), "manifest": manifest}, indent=2))


if __name__ == "__main__":
    main()
