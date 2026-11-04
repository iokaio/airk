# SPDX-License-Identifier: Apache-2.0
"""Admission-order proof must reject real overtaking, without vacuous passes."""
import importlib.util
import contextlib
import copy
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


spec = importlib.util.spec_from_file_location(
    "source_fairness", Path(__file__).resolve().parents[1] / "check-service-source-fairness.py")
fairness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fairness)


def event(sequence, kind, job, job_kind, at):
    return {"sequence": sequence, "kind": kind, "job": job,
            "job_kind": job_kind, "observed_unix_ms": at}


class SourceAdmissionProofTests(unittest.TestCase):
    def test_retained_source_override_requires_explicit_load_only_mode(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            stderr = io.StringIO()
            arguments = ["check-service-source-fairness.py", "--runtime", str(root),
                         "--output", str(root / "output.json"),
                         "--approved-source-manifest", str(root / "old-source.json")]
            with mock.patch("sys.argv", arguments), mock.patch.object(fairness.subprocess, "Popen") as launch:
                with contextlib.redirect_stderr(stderr), self.assertRaises(SystemExit) as refused:
                    fairness.main()
            self.assertEqual(refused.exception.code, 2)
            self.assertIn("explicit --load-only-baseline", stderr.getvalue())
            launch.assert_not_called()

    def test_explicit_baseline_source_is_validated_without_current_source_relabelling(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            (runtime / "airk-runtime.json").write_text("{}", encoding="utf-8")
            approved = {"sha256": "1" * 64, "files": 789}
            with mock.patch.object(fairness.platform, "system", return_value="Windows"), \
                    mock.patch.object(fairness.platform, "machine", return_value="AMD64"), \
                    mock.patch.object(fairness.package, "runtime_builder") as builder, \
                    mock.patch.object(fairness.package, "validate_runtime") as validate, \
                    mock.patch.object(fairness.subprocess, "Popen") as launch:
                selected = fairness.staged_runtime_root(runtime, approved_baseline_source=approved)
            self.assertEqual(selected, runtime.resolve())
            self.assertIs(validate.call_args.args[3], approved)
            builder.assert_not_called()
            launch.assert_not_called()

    def test_baseline_contract_uses_complete_candidate_backlog_range(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            candidate_path, manifest_path = root / "candidate.json", root / "source.json"
            metadata = []
            for index in range(30):
                observed = fairness.search_read_metadata(self.search_response(8 if index < 15 else 9), "fixture")
                observed["semantic_readiness"]["completed_artifacts"] = 128 if index < 15 else 160
                metadata.append(observed)
            candidate = {"status": "passed", "runtime_binary_sha256": "a" * 64,
                         "matched_load": {"source_bytes_unchanged": True, "corpus_files": 387,
                            "corpus_sha256": "c" * 64,
                            "backlog": {"samples_ms": [1.0] * 30, "read_metadata": metadata,
                                        "mode": "lexical", "verification": "off", "limit": 8, "max_bytes": 8192}}}
            manifest_path.write_text(json.dumps({"native_source": {"sha256": "b" * 64, "files": 789}}), encoding="utf-8")
            candidate_path.write_text(json.dumps(candidate), encoding="utf-8")
            contract = fairness.baseline_load_contract(candidate_path, manifest_path, 384)
            self.assertEqual(contract["entry_completed_artifacts"], {"minimum": 128, "maximum": 160})
            self.assertEqual(contract["corpus_sha256"], "c" * 64)
            self.assertEqual(contract["approved_native_source"], {"sha256": "b" * 64, "files": 789})
            candidate["matched_load"]["backlog"]["read_metadata"][-1]["semantic_readiness"] = {
                "state": "ready", "completed_artifacts": 387, "selected_artifacts": 387}
            candidate_path.write_text(json.dumps(candidate), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "Actual native sample 29"):
                fairness.baseline_load_contract(candidate_path, manifest_path, 384)

    def search_response(self, generation=8, state="partial"):
        token = "airk-r1:" + "1" * 32 + f":{generation}:" + "2" * 64
        return {"collection": "fixture", "generation": generation, "publication": {"token": token},
                "semantic_readiness": {"state": state, "selected_artifacts": 387,
                                       "completed_artifacts": 387 if state == "ready" else 32},
                "hits": [{"citation": {"path": "memo.md", "content_hash": "a" * 64}}]}

    def test_source_evidence_requires_advanced_publication_bound_to_its_job(self):
        before = self.search_response(7)["publication"]
        actual = self.search_response(8)
        job = {"outcome": {"state": "succeeded", "revision": actual["publication"]["token"]}}
        metadata = fairness.require_source_publication(actual, before, job, "fixture")
        self.assertIs(metadata["publication"], actual["publication"])
        self.assertIs(metadata["semantic_readiness"], actual["semantic_readiness"])
        # A cached pre-edit snapshot cannot stand in for the actual source read.
        with self.assertRaisesRegex(RuntimeError, "does not advance"):
            fairness.require_source_publication(self.search_response(7), before, job, "fixture")
        fork = copy.deepcopy(actual)
        fork["publication"]["token"] = fork["publication"]["token"][:-64] + "3" * 64
        with self.assertRaisesRegex(RuntimeError, "bind the successful source job"):
            fairness.require_source_publication(fork, before, job, "fixture")

    def test_matched_phase_uses_all_actual_producer_observations_over_cached_status(self):
        metadata = fairness.search_read_metadata(self.search_response(), "fixture")
        samples = {"samples_ms": [1.0] * 30, "read_metadata": [copy.deepcopy(metadata) for _ in range(30)],
                   "cached_summary": {"semantic_readiness": {"state": "ready"}}}
        fairness.require_sample_phase(samples, "backlog", 387)
        samples["read_metadata"][-1] = fairness.search_read_metadata(self.search_response(state="ready"), "fixture")
        samples["cached_summary"]["semantic_readiness"]["state"] = "partial"
        with self.assertRaisesRegex(RuntimeError, "Actual native sample 29"):
            fairness.require_sample_phase(samples, "backlog", 387)
        ready = fairness.search_read_metadata(self.search_response(state="ready"), "fixture")
        samples["read_metadata"] = [copy.deepcopy(ready) for _ in range(30)]
        fairness.require_sample_phase(samples, "quiet", 387)
        samples["read_metadata"][0] = metadata
        samples["cached_summary"]["semantic_readiness"]["state"] = "ready"
        with self.assertRaisesRegex(RuntimeError, "Actual native sample 0"):
            fairness.require_sample_phase(samples, "quiet", 387)

    def test_source_citation_requires_exact_path_and_expected_raw_hash(self):
        bridge = mock.Mock()
        result = self.search_response()
        bridge.read.return_value = result
        self.assertIs(fairness.require_path(bridge, "term", "memo.md", expected_hash="a" * 64), result)
        result["hits"][0]["citation"]["path"] = "other-memo.md"
        result["hits"][0]["text"] = "memo.md appears in payload text only"
        with self.assertRaisesRegex(RuntimeError, "citation mismatch"):
            fairness.require_path(bridge, "term", "memo.md", expected_hash="a" * 64)
        self.assertIs(fairness.require_path(bridge, "term", "memo.md", False), result)
        result["hits"][0]["citation"]["path"] = "memo.md"
        result["hits"][0]["citation"]["content_hash"] = "b" * 64
        with self.assertRaisesRegex(RuntimeError, "citation hash mismatch"):
            fairness.require_path(bridge, "term", "memo.md", expected_hash="a" * 64)

    def test_automatic_source_selects_new_reconcile_not_index_or_old_queued_source(self):
        events = [event(2, "job_pending", "old-source", "reconcile", 900),
                  event(11, "job_running", "old-source", "reconcile", 1100),
                  event(12, "job_pending", "manual-index", "index", 1110),
                  event(13, "job_running", "manual-index", "index", 1120),
                  event(14, "job_pending", "fresh-source", "reconcile", 1130),
                  event(15, "job_running", "fresh-source", "reconcile", 1140)]
        self.assertEqual(fairness.automatic_reconcile_job(events, 1000, 10), "fresh-source")

    def test_source_queued_at_old_sequence_is_excluded_even_with_equal_timestamp(self):
        events = [event(10, "job_pending", "old-source", "reconcile", 1000),
                  event(11, "job_running", "old-source", "reconcile", 1001)]
        self.assertIsNone(fairness.automatic_reconcile_job(events, 1000, 10))

    def test_wave_retains_only_bounded_event_prefix_when_budget_is_exceeded(self):
        record = {"events": [{"sequence": number} for number in range(511)]}
        with self.assertRaisesRegex(RuntimeError, "512-event"):
            fairness.retain_wave_events(record, [{"sequence": 511}, {"sequence": 512}])
        self.assertEqual(len(record["events"]), 512)
        self.assertTrue(record["events_truncated"])
        self.assertEqual(record["unretained_event_count"], 1)

    def test_failed_wave_keeps_original_witness_and_stage_without_a_pass(self):
        fixture = mock.Mock()
        fixture.events.return_value = ([], {"after": 7})
        running = self.vector_observation()
        running["observed_unix_ms"] = 1000
        before = {"publication": {"token": "original-token"},
                  "semantic_readiness": {"completed_artifacts": 32, "selected_artifacts": 387}}
        fixture.running_vectors.return_value = (before, running)
        fixture.watch.side_effect = RuntimeError("fixture metadata observation failed")
        record = {}
        with self.assertRaisesRegex(RuntimeError, "metadata observation failed"):
            fairness.wave(fixture, {"binding": "fixture"}, None, None, {}, "wave", False, 10, record)
        self.assertEqual(record["status"], "failed")
        self.assertEqual(record["stage"], "observe_watch_before_edits")
        self.assertEqual(record["running_vector_before_edits"], running)
        self.assertEqual(record["original_vector_job"], "original")
        self.assertEqual(record["events"], [])
        fixture.enqueue.assert_not_called()

    def vector_observation(self):
        return {"project": "fixture", "project_paused": False,
                "observation": {"current_fencing_generation": 3,
                    "worker": {"project": "fixture", "fencing_generation": 3},
                    "job": {"id": "original", "project": "fixture", "fencing_generation": 3,
                            "kind": "vectors", "state": "running"},
                    "progress": {"job": "original", "phase": "preparing", "paused": False,
                                 "stalled": False, "phase_totals_ms": {"embedding": 187, "preparing": 12700}}}}

    def test_current_grant_post_embedding_preparing_is_a_valid_witness(self):
        value = self.vector_observation()
        original = copy.deepcopy(value)
        self.assertTrue(fairness.active_vector_with_inference(value, "fixture"))
        self.assertEqual(value, original)
        value["observation"]["progress"]["phase"] = "embedding"
        value["observation"]["progress"]["phase_totals_ms"] = {}
        self.assertTrue(fairness.active_vector_with_inference(value, "fixture"))

    def test_foreign_or_stale_progress_is_not_an_active_vector_witness(self):
        for field, replacement in (("job", "foreign-job"), ("fence", 4), ("project", "foreign-project")):
            with self.subTest(field=field):
                value = self.vector_observation()
                if field == "job":
                    value["observation"]["progress"]["job"] = replacement
                elif field == "fence":
                    value["observation"]["current_fencing_generation"] = replacement
                else:
                    value["observation"]["worker"]["project"] = replacement
                self.assertFalse(fairness.active_vector_with_inference(value, "fixture"))

    def test_unproven_inference_and_paused_or_stalled_work_are_refused(self):
        for reason in ("no-inference", "paused", "stalled", "project-paused"):
            with self.subTest(reason=reason):
                value = self.vector_observation()
                work = value["observation"]["progress"]
                if reason == "no-inference":
                    work["phase_totals_ms"]["embedding"] = 0
                elif reason == "project-paused":
                    value["project_paused"] = True
                else:
                    work[reason] = True
                self.assertFalse(fairness.active_vector_with_inference(value, "fixture"))

    def test_near_release_and_nonrunning_jobs_are_refused(self):
        for phase, state in (("retiring", "running"), ("finished", "running"),
                             ("preparing", "succeeded"), ("preparing", "pending")):
            with self.subTest(phase=phase, state=state):
                value = self.vector_observation()
                value["observation"]["progress"]["phase"] = phase
                value["observation"]["job"]["state"] = state
                self.assertFalse(fairness.active_vector_with_inference(value, "fixture"))

    def observed_reader(self):
        reader = fairness.ObservedBridge.__new__(fairness.ObservedBridge)
        reader.responses = fairness.queue.Queue(maxsize=4)
        reader._transport_lock = fairness.threading.Lock()
        reader._transport = {"frame_limit_bytes": fairness.BRIDGE_FRAME_LIMIT,
                             "frames_observed": 0, "max_frame_prefix_bytes": 0, "terminal": None}
        reader._closing = False
        reader.reader = mock.Mock()
        reader.reader.is_alive.return_value = False
        reader.process = mock.Mock()
        reader.process.poll.return_value = None
        return reader

    def test_oversized_frame_signals_immediately_without_capturing_payload(self):
        reader = self.observed_reader()
        payload = b"private-body-" + b"x" * fairness.BRIDGE_FRAME_LIMIT
        stream = io.BytesIO(payload)
        reader._read_frames(stream)
        self.assertEqual(stream.tell(), fairness.BRIDGE_FRAME_LIMIT + 1)
        observation = reader.transport_observation()
        self.assertEqual(observation["terminal"]["kind"], "oversized_frame")
        self.assertEqual(observation["terminal"]["observed_frame_prefix_bytes"], fairness.BRIDGE_FRAME_LIMIT + 1)
        self.assertEqual(observation["frames_observed"], 1)
        self.assertNotIn("private-body", json.dumps(observation))
        self.assertIs(reader.responses.get_nowait(), reader._TERMINAL)
        with self.assertRaises(fairness.BridgeTransportFailure):
            reader.call("tools/call", {})

    def test_eof_and_invalid_json_have_distinct_content_free_terminal_reasons(self):
        for body, kind in ((b"", "eof"), (b'{"private-body":\n', "invalid_json")):
            with self.subTest(kind=kind):
                reader = self.observed_reader()
                reader._read_frames(io.BytesIO(body))
                observation = reader.transport_observation()
                self.assertEqual(observation["terminal"]["kind"], kind)
                self.assertNotIn("private-body", json.dumps(observation))
                self.assertIs(reader.responses.get_nowait(), reader._TERMINAL)

    def test_summary_preserves_the_native_token_only_publication(self):
        token = "airk-r1:" + "1" * 32 + ":7:" + "2" * 64
        value = {"index_state": "available", "collection": "fixture", "generation": 7,
                 "publication": {"token": token}, "files": 387, "chunks": 768,
                 "semantic_readiness": {"state": "partial", "completed_artifacts": 32,
                                        "selected_artifacts": 387}}
        before = json.dumps(value, sort_keys=True)
        self.assertIs(fairness.validate_summary_status(value, "fixture"), value)
        self.assertEqual(json.dumps(value, sort_keys=True), before)
        self.assertEqual(set(value["publication"]), {"token"})
        self.assertEqual(fairness.publication_identity(value["publication"]), ("1" * 32, 7))
        with self.assertRaisesRegex(RuntimeError, "scope/count/publication"):
            fairness.validate_summary_status(value, "foreign")

    def test_full_probe_timeout_with_live_reader_remains_a_blocker(self):
        fixture = mock.Mock()
        probe = fixture.bridge.return_value
        transport = {"terminal": {"kind": "response_timeout"}, "reader_alive": True}
        probe.read.side_effect = fairness.BridgeTransportFailure(transport)
        receipt = {}
        with self.assertRaisesRegex(RuntimeError, "reader alive"):
            fairness.full_diagnostics_probe(fixture, {}, receipt)
        self.assertEqual(receipt["full_diagnostics_probe"]["transport"], transport)
        self.assertEqual(receipt["full_diagnostics_probe"]["outcome"], "transport_failure")
        probe.close.assert_called_once()

    def test_full_probe_records_complete_or_oversized_without_payload(self):
        for oversized in (False, True):
            with self.subTest(oversized=oversized):
                fixture = mock.Mock()
                probe = fixture.bridge.return_value
                transport = {"terminal": {"kind": "oversized_frame"} if oversized else None,
                             "reader_alive": not oversized}
                probe.transport_observation.return_value = transport
                if oversized:
                    probe.read.side_effect = fairness.BridgeTransportFailure(transport)
                else:
                    probe.read.return_value = {"private-body": "must not be persisted"}
                receipt = {}
                fairness.full_diagnostics_probe(fixture, {}, receipt)
                observed = receipt["full_diagnostics_probe"]
                self.assertEqual(observed["outcome"], "transport_failure" if oversized else "complete")
                self.assertNotIn("private-body", json.dumps(observed))
                probe.close.assert_called_once()

    def test_raw_runtime_is_rejected_before_any_native_process_launch(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            (runtime / "airk.exe").write_bytes(b"raw development image")
            output = runtime / "receipt.json"
            stderr = io.StringIO()
            arguments = ["check-service-source-fairness.py", "--runtime", str(runtime),
                         "--output", str(output)]
            with mock.patch("sys.argv", arguments), mock.patch.object(fairness.subprocess, "Popen") as launch:
                with contextlib.redirect_stderr(stderr), self.assertRaises(SystemExit) as refused:
                    fairness.main()
            self.assertEqual(refused.exception.code, 2)
            launch.assert_not_called()
            self.assertIn("airk-runtime.json", stderr.getvalue())
            self.assertIn("scripts/build-service-runtime.py", stderr.getvalue())
            self.assertFalse(output.exists())

    def test_canonical_runtime_rejection_prevents_native_launch(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            (runtime / "airk-runtime.json").write_text("{}", encoding="utf-8")
            output = runtime / "receipt.json"
            stderr = io.StringIO()
            source = {"sha256": "1" * 64, "files": 1}
            builder = mock.Mock()
            builder.source_identity.return_value = source
            arguments = ["check-service-source-fairness.py", "--runtime", str(runtime),
                         "--output", str(output)]
            with mock.patch("sys.argv", arguments), \
                    mock.patch.object(fairness.platform, "system", return_value="Windows"), \
                    mock.patch.object(fairness.platform, "machine", return_value="AMD64"), \
                    mock.patch.object(fairness.package, "runtime_builder", return_value=builder), \
                    mock.patch.object(fairness.package, "validate_runtime", side_effect=ValueError("stale source")) as validate, \
                    mock.patch.object(fairness.subprocess, "Popen") as launch:
                with contextlib.redirect_stderr(stderr), self.assertRaises(SystemExit) as refused:
                    fairness.main()
            self.assertEqual(refused.exception.code, 2)
            self.assertEqual(validate.call_args.args[0], runtime.resolve())
            self.assertEqual(validate.call_args.args[2], "x86_64-pc-windows-msvc")
            self.assertEqual(validate.call_args.args[3], source)
            launch.assert_not_called()
            self.assertIn("stale source", stderr.getvalue())
            self.assertIn("scripts/build-service-runtime.py", stderr.getvalue())
            self.assertFalse(output.exists())

    def test_existing_turn_then_source_and_later_vectors_are_allowed(self):
        events = [event(1, "job_running", "active", "vectors", 900),
                  event(2, "job_succeeded", "active", "vectors", 2500),
                  event(3, "job_running", "source", "index", 2700),
                  event(4, "job_running", "later", "vectors", 4000)]
        proof = fairness.admission_order(events, "source", 1000, "active")
        self.assertEqual(proof["new_vector_overtakers"], 0)
        self.assertEqual(proof["existing_turn_remaining_seconds"], 1.5)
        self.assertAlmostEqual(proof["source_start_excess_over_turn_seconds"], 0.2)

    def test_new_vector_before_source_is_refused(self):
        events = [event(1, "job_running", "other", "vectors", 1001),
                  event(2, "job_running", "source", "index", 2000)]
        with self.assertRaisesRegex(RuntimeError, "overtook"):
            fairness.admission_order(events, "source", 1000, "active")

    def test_no_source_event_cannot_establish_success(self):
        with self.assertRaisesRegex(RuntimeError, "No retained running event"):
            fairness.admission_order([], "source", 1000, None)

    def test_missing_original_completion_cannot_fabricate_zero_remaining(self):
        events = [event(1, "job_running", "source", "index", 2000)]
        with self.assertRaisesRegex(RuntimeError, "unique retained successful completion"):
            fairness.admission_order(events, "source", 1000, "original")

    def test_replacement_vector_cannot_be_relabelled_as_original_turn(self):
        events = [event(1, "job_succeeded", "original", "vectors", 1200),
                  event(2, "job_running", "replacement", "vectors", 1250),
                  event(3, "job_succeeded", "replacement", "vectors", 1500),
                  event(4, "job_running", "source", "index", 1600)]
        with self.assertRaisesRegex(RuntimeError, "overtook"):
            fairness.admission_order(events, "source", 1000, "original")

    def test_source_cannot_precede_original_completion(self):
        events = [event(1, "job_running", "source", "index", 1100),
                  event(2, "job_succeeded", "original", "vectors", 1200)]
        with self.assertRaisesRegex(RuntimeError, "before the Vector completion"):
            fairness.admission_order(events, "source", 1000, "original")

    def test_pre_observation_grant_is_not_claimed_as_an_overtaker(self):
        events = [event(1, "job_succeeded", "original", "vectors", 900),
                  event(2, "job_running", "later", "vectors", 950),
                  event(3, "job_succeeded", "later", "vectors", 1050),
                  event(4, "job_running", "source", "index", 1100)]
        proof = fairness.admission_order(events, "source", 1000, "original",
                                        original_witness_unix_ms=800, original_witness_sequence_floor=0)
        self.assertEqual(proof["new_vector_overtakers"], 0)
        self.assertEqual(proof["active_vector_at_boundary"], "later")
        self.assertEqual(proof["existing_turn_remaining_seconds"], 0.05)

    def test_vector_started_before_admissibility_is_existing_work(self):
        events = [event(1, "job_running", "existing", "vectors", 999),
                  event(2, "job_succeeded", "existing", "vectors", 1300),
                  event(3, "job_running", "source", "index", 1400)]
        proof = fairness.admission_order(events, "source", 1000, "existing")
        self.assertAlmostEqual(proof["source_start_lag_seconds"], 0.4)

    def test_actual_mac_pre_observation_replacement_is_selected_from_durable_interval(self):
        events = [event(26, "job_succeeded", "original", "vectors", 1793749320145),
                  event(27, "job_pending", "replacement", "vectors", 1793749320308),
                  event(28, "job_running", "replacement", "vectors", 1793749320571),
                  event(29, "job_succeeded", "replacement", "vectors", 1793749323367),
                  event(30, "job_pending", "source", "reconcile", 1793749323421),
                  event(31, "job_running", "source", "reconcile", 1793749323685),
                  event(32, "job_succeeded", "source", "reconcile", 1793749324739)]
        proof = fairness.admission_order(events, "source", 1793749322508, "original",
                                        original_witness_unix_ms=1793749318989,
                                        original_witness_sequence_floor=25)
        self.assertEqual(proof["active_vector_at_boundary"], "replacement")
        self.assertAlmostEqual(proof["existing_turn_remaining_seconds"], 0.859)
        self.assertAlmostEqual(proof["source_start_excess_over_turn_seconds"], 0.318)
        original = next(interval for interval in proof["vector_intervals"] if interval["job"] == "original")
        self.assertIsNone(original["running_event"])
        self.assertEqual(original["known_running_lower_bound"]["observed_unix_ms"], 1793749318989)

    def test_no_writer_at_boundary_has_zero_remainder_and_no_later_writer_is_allowed(self):
        events = [event(1, "job_running", "old", "vectors", 800),
                  event(2, "job_succeeded", "old", "vectors", 900),
                  event(3, "job_running", "source", "reconcile", 1100)]
        proof = fairness.admission_order(events, "source", 1000, "old")
        self.assertIsNone(proof["active_vector_at_boundary"])
        self.assertEqual(proof["existing_turn_remaining_seconds"], 0)
        overtaken = events[:2] + [event(3, "job_running", "later", "vectors", 1050),
                                  event(4, "job_succeeded", "later", "vectors", 1080),
                                  event(5, "job_running", "source", "reconcile", 1100)]
        with self.assertRaisesRegex(RuntimeError, "overtook"):
            fairness.admission_order(overtaken, "source", 1000, "old")

    def test_overlapping_writer_intervals_are_refused(self):
        events = [event(1, "job_running", "first", "vectors", 800),
                  event(2, "job_running", "second", "vectors", 900),
                  event(3, "job_succeeded", "first", "vectors", 1100),
                  event(4, "job_succeeded", "second", "vectors", 1200),
                  event(5, "job_running", "source", "index", 1300)]
        with self.assertRaisesRegex(RuntimeError, "Overlapping"):
            fairness.admission_order(events, "source", 1000, "first")

    def test_unknown_interval_or_missing_success_is_refused(self):
        for vector_events in ([event(1, "job_succeeded", "unknown", "vectors", 1200)],
                              [event(1, "job_running", "unfinished", "vectors", 900)]):
            with self.subTest(events=vector_events):
                events = vector_events + [event(2, "job_running", "source", "index", 1300)]
                with self.assertRaisesRegex(RuntimeError, "Unknown Vector interval|successful completion"):
                    fairness.admission_order(events, "source", 1000, None)

    def test_automatic_boundary_equal_to_start_or_end_clock_is_ambiguous(self):
        for start, end in ((1000, 1200), (900, 1000)):
            with self.subTest(start=start, end=end):
                events = [event(1, "job_running", "writer", "vectors", start),
                          event(2, "job_succeeded", "writer", "vectors", end),
                          event(3, "job_running", "source", "index", 1300)]
                with self.assertRaisesRegex(RuntimeError, "Ambiguous equal-time"):
                    fairness.admission_order(events, "source", 1000, "writer")

    def test_manual_pending_sequence_disambiguates_equal_clock_values(self):
        events = [event(1, "job_running", "writer", "vectors", 1000),
                  event(2, "job_pending", "source", "index", 1000),
                  event(3, "job_succeeded", "writer", "vectors", 1000),
                  event(4, "job_running", "source", "index", 1000)]
        proof = fairness.admission_order(events, "source", 1000, "writer", boundary_sequence=2)
        self.assertEqual(proof["active_vector_at_boundary"], "writer")
        self.assertEqual(proof["existing_turn_remaining_seconds"], 0)
        overtaken = [event(1, "job_pending", "source", "index", 1000),
                     event(2, "job_running", "writer", "vectors", 1000),
                     event(3, "job_succeeded", "writer", "vectors", 1000),
                     event(4, "job_running", "source", "index", 1000)]
        with self.assertRaisesRegex(RuntimeError, "overtook"):
            fairness.admission_order(overtaken, "source", 1000, "writer", boundary_sequence=1)


if __name__ == "__main__":
    unittest.main()
