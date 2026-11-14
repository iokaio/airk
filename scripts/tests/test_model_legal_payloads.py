# SPDX-License-Identifier: Apache-2.0
"""Independent byte identities for authenticated, unmodified legal payloads.

Normal unittest discovery checks the small committed payloads. After the resource
window, explicitly bind them to the retained original six-file model input ZIP:
    python scripts/tests/test_model_legal_payloads.py --archive <original.zip>
The archive check streams its existing bytes; it never rebuilds or reseals it.
"""
import hashlib
import json
from pathlib import Path
import sys
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
CC_SOURCE = ('https://raw.githubusercontent.com/creativecommons/cc-legal-tools-data/'
             'a0dac18d1a773b4cdb7d8dd5d9bf4efab7901030/docs/licenses/by/4.0/legalcode.txt')
CC_SHA256 = '9ba9550ad48438d0836ddab3da480b3b69ffa0aac7b7878b5a0039e7ab429411'
MODEL_LICENSE_SHA256 = '9cc7fbd69f78ff19a188025976df20d5c17b0cec5ed79ad5d7e16d2094e46abd'
ARCHIVE_SHA256 = '18f19f92653d98c01f54cce1dacd482de3ece91a635eefa20696de63da8599ea'
ORIGINAL_MEMBERS = (
    'minilm-l6-v2-s8-v2/LICENSE',
    'minilm-l6-v2-s8-v2/model.onnx',
    'minilm-l6-v2-s8-v2/tokenizer.json',
    'minilm-l6-v2-u8-v2/LICENSE',
    'minilm-l6-v2-u8-v2/model.onnx',
    'minilm-l6-v2-u8-v2/tokenizer.json',
)


def verify_original_archive(path):
    """Bind the complete original archive identity and its two legal members."""
    if path.is_symlink() or not path.is_file() or path.stat().st_size != 139684472:
        raise ValueError('Original six-file build archive size/type differs')
    with path.open('rb') as source:
        observed = hashlib.file_digest(source, 'sha256').hexdigest()
    if observed != ARCHIVE_SHA256:
        raise ValueError('Original build archive bytes differ; do not reseal')
    legal = (ROOT/'third_party/models/minilm-v2-LICENSE.txt').read_bytes()
    if len(legal) != 34889 or hashlib.sha256(legal).hexdigest() != MODEL_LICENSE_SHA256:
        raise ValueError('Committed trained-model legal payload differs')
    with zipfile.ZipFile(path) as archive:
        if tuple(sorted(archive.namelist())) != ORIGINAL_MEMBERS:
            raise ValueError('Original six-file inventory differs')
        for name in ('minilm-l6-v2-s8-v2/LICENSE','minilm-l6-v2-u8-v2/LICENSE'):
            if archive.read(name) != legal:
                raise ValueError('Original bundled license differs: ' + name)
    return {'archive_sha256':observed,'bytes':139684472,'files':list(ORIGINAL_MEMBERS),
            'license_sha256':MODEL_LICENSE_SHA256,'license_bytes':34889,
            'scope':'Original archive and legal-byte custody only; no model qualification'}


class AuthenticatedLegalPayloads(unittest.TestCase):
    def test_entire_payload_bytes_and_trusted_cc_revision_are_frozen(self):
        # Literal expectations are independent of license-notices.py's mapping.
        for name, size, expected in (
            ('CC-BY-4.0.txt',18657,CC_SHA256),
            ('minilm-v2-LICENSE.txt',34889,MODEL_LICENSE_SHA256),
        ):
            with self.subTest(name=name):
                path = ROOT/'third_party/models'/name
                self.assertFalse(path.is_symlink())
                content = path.read_bytes()
                self.assertEqual(len(content),size)
                self.assertEqual(hashlib.sha256(content).hexdigest(),expected)
        provenance = json.loads((ROOT/'third_party/models/minilm-v2-provenance.json').read_bytes())
        self.assertEqual(provenance['cc_by_legal_text'], {
            'file':'CC-BY-4.0.txt','sha256':CC_SHA256,
            'canonical_url':'https://creativecommons.org/licenses/by/4.0/legalcode',
            'source_url':CC_SOURCE,
        })
        self.assertEqual(provenance['shared_assets']['license_sha256'],MODEL_LICENSE_SHA256)

    def test_only_the_two_authenticated_legal_paths_have_the_eof_exception(self):
        lines = (ROOT/'.gitattributes').read_text(encoding='utf-8').splitlines()
        self.assertIn('* text=auto eol=lf',lines)
        whitespace_rules = {
            line for line in lines if line.strip() and not line.lstrip().startswith('#')
            and any(token.lstrip('-!').split('=',1)[0] == 'whitespace'
                    for token in line.split()[1:])
        }
        self.assertEqual(whitespace_rules, {
            'third_party/models/CC-BY-4.0.txt -text whitespace=-blank-at-eof',
            'third_party/models/minilm-v2-LICENSE.txt -text whitespace=-blank-at-eof',
        })


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--archive':
        print(json.dumps(verify_original_archive(Path(sys.argv[2])),indent=2))
    else:
        unittest.main()
