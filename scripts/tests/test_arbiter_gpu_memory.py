# SPDX-License-Identifier: Apache-2.0
"""Portable injected controls; no Windows API, adapter, GPU or model is activated."""
import ctypes
import importlib.util
from pathlib import Path
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "arbiter_gpu_memory_controls", Path(__file__).resolve().parents[1] / "arbiter-gpu-memory.py")
memory = importlib.util.module_from_spec(SPEC)
# Exercise the repository's existing loader pattern without sys.modules insertion.
SPEC.loader.exec_module(memory)


class Clock:
    def __init__(self):
        self.value = 0.0
    def __call__(self):
        return self.value


class Api:
    def __init__(self, clock):
        self.clock = clock
        self.calls = []
        self.states = []
        self.open_result = (0, 119)
        self.close_status = 0
        self.statuses = {0: 0, 1: 0}
        self.usage = {0: 10, 1: 20}
        self.query_hook = None
    def process_state(self, handle):
        self.calls.append(("identity", handle))
        value = self.states.pop(0) if self.states else memory.ProcessState(memory.ProcessIdentity(321, 456), True)
        if isinstance(value, Exception):
            raise value
        return value
    def open_adapter(self, identity):
        self.calls.append(("open", identity))
        return self.open_result
    def query(self, process, adapter, physical_index, group):
        self.calls.append(("query", process, adapter, physical_index, group))
        if self.query_hook:
            self.query_hook(group)
        self.clock.value += 0.1
        return self.statuses[group], dict(zip(memory.VALUE_FIELDS, (1000, self.usage[group], 100, 900)))
    def close_adapter(self, handle):
        self.calls.append(("close_adapter", handle))
        return self.close_status


def fixture(deadline=10):
    clock = Clock()
    api = Api(clock)
    process = memory.ProcessIdentity(321, 456)
    adapter = memory.AdapterIdentity(0x12345678, -1, 3)
    sampler = memory.Sampler(api, 123, process, adapter, deadline, clock=clock)
    return sampler, api, clock


class GpuMemoryTests(unittest.TestCase):
    def test_sdk_layouts_are_exact_for_32_and_64_bit(self):
        self.assertEqual(ctypes.sizeof(memory.Luid), 8)
        self.assertEqual(ctypes.sizeof(memory.OpenAdapter), 12)
        self.assertEqual(ctypes.sizeof(memory.CloseAdapter), 4)
        expected = {"Budget": 16, "CurrentUsage": 24, "CurrentReservation": 32,
                    "AvailableForReservation": 40, "PhysicalAdapterIndex": 48}
        for cls, adapter_offset, group_offset in (
            (memory.QueryVideoMemoryInfo32, 4, 8),
            (memory.QueryVideoMemoryInfo64, 8, 12),
        ):
            self.assertEqual(ctypes.sizeof(cls), 56)
            self.assertEqual(cls.hProcess.offset, 0)
            self.assertEqual(cls.hAdapter.offset, adapter_offset)
            self.assertEqual(cls.MemorySegmentGroup.offset, group_offset)
            for field, offset in expected.items():
                self.assertEqual(getattr(cls, field).offset, offset)

    def test_import_and_sampler_construction_do_not_bind_or_probe(self):
        with mock.patch.object(ctypes, "WinDLL", create=True) as bind:
            spec = importlib.util.spec_from_file_location("unregistered_memory_import", SPEC.origin)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            sampler, api, _ = fixture()
            self.assertEqual(api.calls, [])
            self.assertIsNone(sampler.summary()["lifetime_peak_bytes"])
            bind.assert_not_called()

    def test_valid_identity_fenced_samples_and_exact_adapter_arguments(self):
        sampler, api, clock = fixture()
        result = sampler.sample()
        self.assertTrue(result["valid"])
        self.assertEqual(result["segments"]["local"]["values"]["current_usage_bytes"], 10)
        self.assertEqual(result["segments"]["non_local"]["values"]["current_usage_bytes"], 20)
        self.assertEqual([call for call in api.calls if call[0] == "query"],
                         [("query", 123, 119, 3, 0), ("query", 123, 119, 3, 1)])
        clock.value = 2.0
        api.usage = {0: 5, 1: 30}
        second = sampler.sample()
        self.assertEqual(second["start_gap_seconds"], 2.0)
        summary = sampler.summary()
        self.assertEqual(summary["sampled_max_current_usage_bytes"], {"local": 10, "non_local": 30})
        self.assertEqual(summary["maximum_start_gap_seconds"], 2.0)
        self.assertEqual(summary["valid_sample_count"], 2)
        self.assertEqual(len([c for c in api.calls if c[0] == "open"]), 1)
        self.assertIsNone(summary["lifetime_peak_bytes"])
        self.assertFalse(summary["admission_authority"])
        self.assertFalse(summary["final_gpu_qualification"])

    def test_valid_zero_is_distinct_from_unavailable(self):
        sampler, api, _ = fixture()
        api.usage = {0: 0, 1: 0}
        self.assertTrue(sampler.sample()["valid"])
        self.assertEqual(sampler.summary()["sampled_max_current_usage_bytes"], {"local": 0, "non_local": 0})
        failed, denied, _ = fixture()
        denied.statuses[0] = -1073741790
        result = failed.sample()
        self.assertFalse(result["valid"])
        self.assertEqual(result["segments"]["local"]["ntstatus"], -1073741790)
        self.assertIsNone(result["segments"]["local"]["values"])
        self.assertEqual(failed.summary()["sampled_max_current_usage_bytes"], {"local": None, "non_local": None})

    def test_nonzero_including_pending_status_never_means_observed_zero(self):
        for status in (1, 0x103, -1073741811):
            sampler, api, _ = fixture()
            api.statuses[1] = status
            result = sampler.sample()
            self.assertFalse(result["valid"])
            self.assertEqual(result["segments"]["non_local"]["ntstatus"], status)
            self.assertIsNone(result["segments"]["non_local"]["values"])
            self.assertEqual(sampler.summary()["unknown_sample_count"], 1)

    def test_adapter_open_failure_is_retained_without_retry_or_queries(self):
        sampler, api, _ = fixture()
        api.open_result = (-1073741790, 0)
        for _ in range(2):
            result = sampler.sample()
            self.assertFalse(result["valid"])
            self.assertEqual(result["adapter_open_status"], -1073741790)
        self.assertEqual(len([c for c in api.calls if c[0] == "open"]), 1)
        self.assertEqual(len([c for c in api.calls if c[0] == "query"]), 0)

    def test_success_with_invalid_adapter_handle_still_refuses(self):
        sampler, api, _ = fixture()
        api.open_result = (0, 0)
        result = sampler.sample()
        self.assertFalse(result["valid"])
        self.assertEqual(result["adapter_open_status"], 0)
        self.assertFalse(any(c[0] == "query" for c in api.calls))

    def test_wrong_pid_creation_or_exited_before_sample_never_queries(self):
        for state in (
            memory.ProcessState(memory.ProcessIdentity(999, 456), True),
            memory.ProcessState(memory.ProcessIdentity(321, 999), True),
            memory.ProcessState(memory.ProcessIdentity(321, 456), False),
            PermissionError("query denied"),
        ):
            sampler, api, _ = fixture()
            api.states = [state]
            self.assertFalse(sampler.sample()["valid"])
            self.assertFalse(any(c[0] in ("open", "query") for c in api.calls))

    def test_identity_or_exit_race_retains_raw_readings_but_does_not_update_maximum(self):
        for state in (
            memory.ProcessState(memory.ProcessIdentity(321, 999), True),
            memory.ProcessState(memory.ProcessIdentity(321, 456), False),
        ):
            sampler, api, _ = fixture()
            api.states = [memory.ProcessState(memory.ProcessIdentity(321, 456), True), state]
            result = sampler.sample()
            self.assertFalse(result["valid"])
            self.assertEqual(result["segments"]["local"]["values"]["current_usage_bytes"], 10)
            self.assertEqual(sampler.summary()["sampled_max_current_usage_bytes"], {"local": None, "non_local": None})

    def test_deadline_expired_before_sample_dispatches_no_api(self):
        sampler, api, clock = fixture()
        clock.value = 10.0
        self.assertFalse(sampler.sample()["valid"])
        self.assertEqual(api.calls, [])

    def test_late_callback_never_renews_deadline_or_validates_reading(self):
        sampler, api, clock = fixture()
        api.query_hook = lambda group: setattr(clock, "value", 11.0)
        result = sampler.sample()
        self.assertFalse(result["valid"])
        self.assertEqual(sampler.deadline, 10.0)
        self.assertIsNotNone(result["segments"]["local"]["values"])
        self.assertFalse(any(c[0] == "query" and c[-1] == 1 for c in api.calls))
        self.assertIsNone(sampler.summary()["sampled_max_current_usage_bytes"]["local"])

    def test_clock_regression_and_nonfinite_readings_remain_unknown(self):
        sampler, api, clock = fixture()
        sampler.sample()
        clock.value = 0.0
        self.assertFalse(sampler.sample()["valid"])
        other, _, bad_clock = fixture()
        bad_clock.value = float("nan")
        result = other.sample()
        self.assertFalse(result["valid"])
        self.assertIsNone(result["started_at"])
        self.assertIsNone(result["finished_at"])

    def test_query_exception_does_not_hide_other_segment_status(self):
        sampler, api, _ = fixture()
        def fail(group):
            if group == 0:
                raise PermissionError("local query refused")
        api.query_hook = fail
        result = sampler.sample()
        self.assertFalse(result["valid"])
        self.assertEqual(result["segments"]["local"]["error"]["type"], "PermissionError")
        self.assertEqual(result["segments"]["non_local"]["ntstatus"], 0)

    def test_negative_or_malformed_counter_data_refuses(self):
        sampler, api, _ = fixture()
        api.usage[0] = -1
        self.assertFalse(sampler.sample()["valid"])
        self.assertIsNone(sampler.summary()["sampled_max_current_usage_bytes"]["local"])

    def test_bindings_are_read_only_and_invalid_configuration_fails_before_api(self):
        sampler, api, _ = fixture()
        for name, value in (("deadline", 100), ("process_handle", 456), ("process", None), ("adapter", None)):
            with self.assertRaises(AttributeError):
                setattr(sampler, name, value)
        for handle in (0, -1, True):
            with self.assertRaises(ValueError):
                memory.Sampler(api, handle, sampler.process, sampler.adapter, 10, clock=lambda: 0)
        with self.assertRaises(ValueError):
            memory.Sampler(api, 123, sampler.process, sampler.adapter, 0, clock=lambda: 0)
        with self.assertRaises(ValueError):
            memory.AdapterIdentity(1 << 32, 0, 0)
        self.assertEqual(api.calls, [])

    def test_close_owns_only_adapter_and_never_retries_or_queries_after_close(self):
        sampler, api, _ = fixture()
        sampler.sample()
        first = sampler.close()
        self.assertTrue(first["closed"])
        self.assertEqual(first["adapter_close_status"], 0)
        self.assertFalse(first["process_handle_closed"])
        self.assertEqual(sampler.close(), first)
        self.assertEqual(len([c for c in api.calls if c[0] == "close_adapter"]), 1)
        count = len(api.calls)
        self.assertFalse(sampler.sample()["valid"])
        self.assertEqual(len(api.calls), count)

    def test_failed_adapter_cleanup_is_not_manufactured_or_retried(self):
        sampler, api, _ = fixture()
        sampler.sample()
        api.close_status = -1073741790
        result = sampler.close()
        self.assertFalse(result["closed"])
        self.assertEqual(result["adapter_close_status"], -1073741790)
        self.assertEqual(sampler.close(), result)
        self.assertEqual(len([c for c in api.calls if c[0] == "close_adapter"]), 1)

    def test_close_without_open_never_closes_the_child(self):
        sampler, api, _ = fixture()
        result = sampler.close()
        self.assertTrue(result["closed"])
        self.assertFalse(result["adapter_close_attempted"])
        self.assertFalse(result["process_handle_closed"])
        self.assertEqual(api.calls, [])

    def test_real_api_binding_and_struct_arguments_through_injected_functions_only(self):
        class Function:
            def __init__(self, call):
                self.call = call
            def __call__(self, *args):
                return self.call(*args)
        class Library:
            pass
        gdi, kernel = Library(), Library()
        seen = []
        def open_adapter(pointer):
            value = pointer._obj
            seen.append(("open", value.AdapterLuid.LowPart, value.AdapterLuid.HighPart))
            value.hAdapter = 119
            return 0
        def query(pointer):
            value = pointer._obj
            seen.append(("query", value.hProcess, value.hAdapter, value.MemorySegmentGroup,
                         value.PhysicalAdapterIndex))
            value.Budget, value.CurrentUsage = 1000, 44
            value.CurrentReservation, value.AvailableForReservation = 100, 900
            return 0
        gdi.D3DKMTOpenAdapterFromLuid = Function(open_adapter)
        gdi.D3DKMTQueryVideoMemoryInfo = Function(query)
        gdi.D3DKMTCloseAdapter = Function(lambda p: seen.append(("close", p._obj.hAdapter)) or 0)
        kernel.GetProcessId = Function(lambda h: 321)
        def process_times(handle, created, exited, kernel_time, user_time):
            created._obj.low = 456
            return 1
        kernel.GetProcessTimes = Function(process_times)
        kernel.WaitForSingleObject = Function(lambda h, timeout: 258)
        with mock.patch.object(memory.os, "name", "nt"), mock.patch.object(memory.platform, "machine", return_value="AMD64"), \
                mock.patch.object(ctypes, "WinDLL", create=True, side_effect=[gdi, kernel]) as dll:
            api = memory.WindowsApi()
            self.assertEqual(api.process_state(123), memory.ProcessState(memory.ProcessIdentity(321, 456), True))
            self.assertEqual(api.open_adapter(memory.AdapterIdentity(8, -1, 3)), (0, 119))
            self.assertEqual(api.query(123, 119, 3, 1), (0, dict(zip(memory.VALUE_FIELDS, (1000, 44, 100, 900)))))
            self.assertEqual(api.close_adapter(119), 0)
        self.assertEqual([c.args[0] for c in dll.call_args_list], ["gdi32.dll", "kernel32.dll"])
        self.assertTrue(all(c.kwargs["winmode"] == 0x800 for c in dll.call_args_list))
        self.assertEqual(seen, [("open", 8, -1), ("query", 123, 119, 1, 3), ("close", 119)])
        self.assertIs(gdi.D3DKMTQueryVideoMemoryInfo.restype, ctypes.c_int32)

    def test_unsupported_host_refuses_before_binding_any_dll(self):
        with mock.patch.object(memory.os, "name", "posix"), mock.patch.object(ctypes, "WinDLL", create=True) as dll:
            with self.assertRaises(OSError):
                memory.WindowsApi()
            dll.assert_not_called()


if __name__ == "__main__":
    unittest.main()
