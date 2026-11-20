# SPDX-License-Identifier: Apache-2.0
"""Model-free Hub controls with injected HTTP responses and SDK operations."""

from email.message import Message
import hashlib
import io
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import Mock


sys.path.insert(0, str(Path(__file__).resolve().parent))
from hf_transport import CARD, HubReleaseError, REPO, publish_hub, verify_hub


HEAD = "1" * 40
NEXT = "2" * 40
CREDENTIAL = "synthetic-hub-token"
URL = f"https://huggingface.co/{REPO}"
A = "airk-arbiter-focus-r001.model.onnx"
B = "airk-arbiter-focus-r001.compiler.json"
LOCAL = Path(__file__).resolve()


def resolve(commit, leaf):
    return f"{URL}/resolve/{commit}/{leaf}"


def pin(leaf, data, *, local=False):
    row = {"path": leaf, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    if local:
        row["local_path"] = str(LOCAL)
    return row


class Response(io.BytesIO):
    def __init__(self, code, content=b"", headers=()):
        super().__init__(content)
        self.code = code
        self.headers = Message()
        self.read_sizes = []
        for name, value in headers or ():
            self.headers[name] = value
        if code == 200 and headers is not None and "Content-Length" not in self.headers:
            self.headers["Content-Length"] = str(len(content))

    def getcode(self):
        return self.code

    def read(self, size=-1):
        self.read_sizes.append(size)
        return super().read(size)


class Readback:
    def __init__(self, *responses):
        self.responses = iter(responses)
        self.urls = []
        self.requests = []

    def __call__(self, request, *, timeout):
        self.urls.append(request.full_url)
        self.requests.append(request)
        return next(self.responses)


class VerificationTests(unittest.TestCase):
    def test_anonymous_streaming_preserves_order_and_exact_pins(self):
        content = b"abcdef"
        first = Response(200, content, headers=[("Content-Length", "6")])
        second = Response(200, content[::-1])
        readback = Readback(first, second)
        pins = [pin(A, content), pin(B, content[::-1])]
        self.assertEqual(verify_hub(REPO, HEAD, pins, opener=readback),
                         {"repo": REPO, "commit": HEAD, "files": pins})
        self.assertEqual(readback.urls, [resolve(HEAD, A), resolve(HEAD, B)])
        for request in readback.requests:
            self.assertFalse(request.has_header("Authorization"))
            self.assertFalse(request.has_header("Cookie"))
            self.assertEqual(request.get_header("Accept-encoding"), "identity")
        self.assertEqual(first.read_sizes, [7, 1])
        self.assertTrue(first.closed and second.closed)

    def test_commit_and_flat_paths_are_required_before_network(self):
        opener = Mock()
        data = b"x"
        cases = [("iokaio/other-models", HEAD, [pin(A, data)]), (REPO, "main", [pin(A, data)]),
                 (REPO, HEAD, [pin("https://example.invalid/extra", data)]),
                 (REPO, HEAD, [pin("../up.onnx", data)]), (REPO, HEAD, [pin(".gitattributes", data)]),
                 (REPO, HEAD, [pin(A, data), pin(A, data)]),
                 (REPO, HEAD, [dict(pin(A, data), bytes=(1 << 30) + 1)])]
        for case in cases:
            with self.subTest(case=case), self.assertRaises(HubReleaseError):
                verify_hub(*case, opener=opener)
        opener.assert_not_called()

    def test_exact_resolve_cache_and_native_xet_route(self):
        content = b"xet"
        cache = f"/api/resolve-cache/models/{REPO}/{HEAD}/{A}?etag=opaque"
        edge = "https://us.aws.cdn.hf.co/xet-bridge-us/" + "a"*24 + "/" + "b"*64 + "?signature=opaque"
        redirects = [Response(307, headers=[("Location", cache)]), Response(302, headers=[("Location", edge)])]
        readback = Readback(*redirects, Response(200, content))
        self.assertEqual(verify_hub(REPO, HEAD, [pin(A, content)], opener=readback)["commit"], HEAD)
        self.assertEqual(readback.urls, [resolve(HEAD, A), "https://huggingface.co" + cache, edge])
        self.assertTrue(all(response.closed for response in redirects))

    def test_foreign_changed_or_ambiguous_redirects_are_never_fetched(self):
        candidate = pin(A, b"x")
        locations = ["//evil.invalid/f", "http://huggingface.co/f", "https://huggingface.co.evil.invalid/f",
                     "https://user@huggingface.co/f", "https://huggingface.co:443/f",
                     resolve(NEXT, A), resolve(HEAD, B), resolve(HEAD, A) + "#fragment",
                     "https://us.aws.cdn.hf.co/arbitrary/object?signature=secret"]
        for target in locations:
            with self.subTest(target=target):
                readback = Readback(Response(302, headers=[("Location", target)]))
                with self.assertRaises(HubReleaseError) as error:
                    verify_hub(REPO, HEAD, [candidate], opener=readback)
                self.assertEqual(readback.urls, [resolve(HEAD, A)])
                self.assertNotIn("signature=secret", str(error.exception))

    def test_redirect_budget_covers_the_entire_file(self):
        replies = [Response(302, headers=[("Location", resolve(HEAD, A))]) for _ in range(5)]
        readback = Readback(*replies)
        with self.assertRaisesRegex(HubReleaseError, "redirect limit"):
            verify_hub(REPO, HEAD, [pin(A, b"x")], opener=readback)
        self.assertEqual(len(readback.urls), 5)

    def test_lengths_encoding_hash_and_partial_responses_fail_closed(self):
        cases = [
            (Response(200, b"aa", headers=[("Content-Length", "2")]), "Content-Length differs"),
            (Response(200, b"aa", headers=[("Content-Length", "1")]), "exceeded"),
            (Response(200, b"", headers=[("Content-Length", "1")]), "length or SHA-256"),
            (Response(200, b"a", headers=None), "Content-Length differs"),
            (Response(200, b"b"), "length or SHA-256"),
            (Response(206, b"a"), "HTTP 206"),
            (Response(429, b"ignored"), "HTTP 429"),
            (Response(200, b"a", headers=[("Content-Encoding", "gzip")]), "encoded response"),
            (Response(200, b"a", headers=[("Content-Range", "bytes 0-0/2")]), "partial file"),
            (Response(200, b"a", headers=[("Content-Length", "1"), ("Content-Length", "1")]),
             "repeated Content-Length"),
        ]
        for response, reason in cases:
            with self.subTest(reason=reason), self.assertRaisesRegex(HubReleaseError, reason):
                verify_hub(REPO, HEAD, [pin(A, b"a")], opener=Readback(response))
            self.assertTrue(response.closed)


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.content = LOCAL.read_bytes()
        self.local = [pin(A, self.content, local=True), pin(B, self.content, local=True)]
        self.remote = [pin(A, self.content), pin(B, self.content)]
        self.api = Mock()
        self.api.model_info.return_value = SimpleNamespace(id=REPO, private=False, gated=False, sha=HEAD)
        self.api.list_repo_files.return_value = [".gitattributes"]
        self.api.create_commit.return_value = SimpleNamespace(oid=NEXT, commit_url=f"{URL}/commit/{NEXT}")
        self.add = Mock(side_effect=self.operation)

    def operation(self, *, path_in_repo, path_or_fileobj):
        self.assertIsInstance(path_or_fileobj, Path)
        data = path_or_fileobj.read_bytes()
        return SimpleNamespace(path_in_repo=path_in_repo, path_or_fileobj=path_or_fileobj,
                               upload_info=SimpleNamespace(size=len(data), sha256=hashlib.sha256(data).digest()))

    def publish(self, *, card=None, readback=None, head=HEAD, credential=CREDENTIAL, repo=REPO):
        if readback is None:
            readback = Readback()
        return publish_hub(repo, self.local, expected_head=head, credential=credential, card=card,
                           api=self.api, add_operation=self.add, opener=readback)

    def test_partial_upload_is_reconciled_and_only_missing_paths_committed(self):
        self.api.list_repo_files.return_value += [A]
        readback = Readback(*(Response(200, self.content) for _ in range(3)))
        self.assertEqual(self.publish(readback=readback),
                         {"repo": REPO, "commit": NEXT, "files": self.remote, "changed": True})
        self.assertEqual(readback.urls, [resolve(HEAD, A), resolve(NEXT, A), resolve(NEXT, B)])
        self.api.model_info.assert_called_once_with(REPO, revision="main", token=False)
        self.api.list_repo_files.assert_called_once_with(REPO, repo_type="model", revision=HEAD, token=False)
        arguments = self.api.create_commit.call_args.kwargs
        self.assertEqual((arguments["repo_id"], arguments["repo_type"], arguments["revision"],
                          arguments["parent_commit"], arguments["token"], arguments["create_pr"]),
                         (REPO, "model", "main", HEAD, CREDENTIAL, False))
        self.assertEqual([operation.path_in_repo for operation in arguments["operations"]], [B])
        if sys.platform == "win32":
            self.assertEqual(arguments["operations"][0].path_or_fileobj, LOCAL)

    def test_existing_match_is_read_once_and_needs_no_commit(self):
        self.api.list_repo_files.return_value += [A, B]
        readback = Readback(Response(200, self.content), Response(200, self.content))
        self.assertEqual(self.publish(readback=readback),
                         {"repo": REPO, "commit": HEAD, "files": self.remote, "changed": False})
        self.assertEqual(readback.urls, [resolve(HEAD, A), resolve(HEAD, B)])
        self.add.assert_not_called()
        self.api.create_commit.assert_not_called()

    def test_existing_conflict_is_verified_by_content_and_never_replaced(self):
        self.api.list_repo_files.return_value += [A]
        readback = Readback(Response(200, b"X"*len(self.content)))
        with self.assertRaisesRegex(HubReleaseError, "existing immutable Hub filename.*SHA-256"):
            self.publish(readback=readback)
        self.assertEqual(readback.urls, [resolve(HEAD, A)])
        self.add.assert_not_called()
        self.api.create_commit.assert_not_called()

    def test_reviewed_card_is_explicit_and_verified_as_its_own_pin(self):
        card = pin(CARD, self.content, local=True)
        readback = Readback(*(Response(200, self.content) for _ in range(3)))
        result = self.publish(card=card, readback=readback)
        self.assertEqual(result["files"], self.remote + [pin(CARD, self.content)])
        self.assertEqual([operation.path_in_repo for operation in self.api.create_commit.call_args.kwargs["operations"]],
                         [A, B, CARD])
        with self.assertRaisesRegex(HubReleaseError, "reviewed README"):
            self.publish(card=pin("other.txt", self.content, local=True))

    def test_existing_card_with_other_bytes_is_never_replaced(self):
        self.api.list_repo_files.return_value += [CARD]
        with self.assertRaisesRegex(HubReleaseError, "README.md.*SHA-256"):
            self.publish(card=pin(CARD, self.content, local=True),
                         readback=Readback(Response(200, b"X"*len(self.content))))
        self.api.create_commit.assert_not_called()

    def test_wrong_repo_changed_head_gated_repo_and_path_collision_never_commit(self):
        with self.assertRaisesRegex(HubReleaseError, "only .* authorized"):
            self.publish(repo="other/project")
        self.api.model_info.assert_not_called()
        with self.assertRaisesRegex(HubReleaseError, "reviewed parent"):
            self.publish(head=NEXT)
        self.api.model_info.return_value.gated = "auto"
        with self.assertRaisesRegex(HubReleaseError, "public and ungated"):
            self.publish()
        self.api.model_info.return_value.gated = False
        self.api.list_repo_files.return_value += [A.upper()]
        with self.assertRaisesRegex(HubReleaseError, "path conflicts"):
            self.publish()
        self.add.assert_not_called()
        self.api.create_commit.assert_not_called()

    def test_bad_local_and_sdk_pins_never_commit(self):
        replace = self.local[1]["sha256"]
        self.local[1]["sha256"] = "f"*64
        with self.assertRaisesRegex(HubReleaseError, "staged file pin differs"):
            self.publish()
        self.api.model_info.assert_not_called()
        self.local[1]["sha256"] = replace
        self.add.side_effect = lambda **_: SimpleNamespace(upload_info=SimpleNamespace(size=0, sha256=b"bad"))
        with self.assertRaisesRegex(HubReleaseError, "SDK upload bytes differ"):
            self.publish()
        self.api.create_commit.assert_not_called()

    def test_commit_error_redacts_credentials_and_requires_reconciliation(self):
        self.api.create_commit.side_effect = RuntimeError("sensitive request details " + CREDENTIAL)
        with self.assertRaisesRegex(HubReleaseError, "inspect main and reconcile") as error:
            self.publish()
        self.assertNotIn(CREDENTIAL, str(error.exception))
        self.assertTrue(error.exception.__suppress_context__)
        self.api.create_commit.assert_called_once()

    def test_anonymous_readback_failure_identifies_the_confirmed_commit(self):
        with self.assertRaisesRegex(HubReleaseError, f"Hub commit {NEXT} exists but anonymous verification failed"):
            self.publish(readback=Readback(Response(200, b"bad")))
        self.api.create_commit.assert_called_once()


if __name__ == "__main__":
    unittest.main()
