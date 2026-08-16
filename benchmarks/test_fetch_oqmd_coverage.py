#!/usr/bin/env python3
"""Self-check for fetch_oqmd_coverage.py's resume-cache and retry logic.
Run: python3 benchmarks/test_fetch_oqmd_coverage.py -v

No real HTTP request is made -- query_composition's network layer
(_urlopen) is monkeypatched, and time.sleep is stubbed so retry-backoff
tests run instantly. Exercises the scenario matrix requested when the
resume cache moved from "convenient" to "the thing a multi-run coverage
fetch's honesty depends on": tail-corruption recovery vs. real
corruption, cache fingerprinting, the schema-validation bug where an
empty first response permanently skipped validation, and the transient
vs. non-transient retry boundary.
"""

import json
import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest import mock

import fetch_oqmd_coverage as foc


def _row(formula, data=None, meta=None, fetched_at_utc="2026-08-16T00:00:00+00:00"):
    row = {"formula": formula, "data": data if data is not None else [], "meta": meta if meta is not None else {}}
    if fetched_at_utc is not None:
        row["fetched_at_utc"] = fetched_at_utc
    return row


class SelectPolymorphTests(unittest.TestCase):
    """Regression coverage for the "preferred entry" bug: OQMD's own docs
    define duplicate_entry_id as the ID of the *preferred* entry sharing a
    crystal structure, not a duplicate/not-duplicate flag. Self-referencing
    (duplicate_entry_id == entry_id) and null both mean "keep me"; only a
    duplicate_entry_id pointing at a *different* entry_id means exclude."""

    def test_self_referencing_entries_are_kept_not_excluded(self):
        entries = [
            {"entry_id": 1, "duplicate_entry_id": 1, "delta_e": -1.0, "natoms": 2, "volume": 10.0},
            {"entry_id": 2, "duplicate_entry_id": 2, "delta_e": -2.0, "natoms": 2, "volume": 10.0},
        ]
        chosen, n_excluded, n_null = foc.select_polymorph(entries)
        self.assertIsNotNone(chosen)
        self.assertEqual(chosen["entry_id"], 2)  # lowest delta_e among the (both-preferred) candidates
        self.assertEqual(n_excluded, 0)

    def test_null_duplicate_entry_id_is_kept(self):
        entries = [{"entry_id": 5, "duplicate_entry_id": None, "delta_e": -0.5, "natoms": 2, "volume": 10.0}]
        chosen, n_excluded, n_null = foc.select_polymorph(entries)
        self.assertIsNotNone(chosen)
        self.assertEqual(n_excluded, 0)

    def test_entry_pointing_at_a_different_entry_is_excluded(self):
        entries = [
            {"entry_id": 1, "duplicate_entry_id": 1, "delta_e": -1.0, "natoms": 2, "volume": 10.0},  # preferred
            {"entry_id": 2, "duplicate_entry_id": 1, "delta_e": -99.0, "natoms": 2, "volume": 10.0},  # non-preferred dup of 1
        ]
        chosen, n_excluded, n_null = foc.select_polymorph(entries)
        self.assertIsNotNone(chosen)
        self.assertEqual(chosen["entry_id"], 1)
        self.assertEqual(n_excluded, 1)

    def test_all_non_preferred_yields_no_match(self):
        # Every entry points at some other entry_id not present in this list --
        # nothing here is preferred, so there is nothing to choose.
        entries = [
            {"entry_id": 2, "duplicate_entry_id": 1, "delta_e": -1.0, "natoms": 2, "volume": 10.0},
            {"entry_id": 3, "duplicate_entry_id": 1, "delta_e": -2.0, "natoms": 2, "volume": 10.0},
        ]
        chosen, n_excluded, n_null = foc.select_polymorph(entries)
        self.assertIsNone(chosen)
        self.assertEqual(n_excluded, 2)


class LoadCacheTests(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.cache_path = Path(self._tmp.name) / "cache.jsonl"

    def tearDown(self):
        self._tmp.cleanup()

    def test_missing_file_returns_empty(self):
        self.assertEqual(foc._load_cache(self.cache_path), {})

    def test_normal_resume(self):
        rows = [_row("TiO2"), _row("Nb2O5")]
        self.cache_path.write_text("\n".join(json.dumps(r) for r in rows) + "\n")
        cached = foc._load_cache(self.cache_path)
        self.assertEqual(set(cached), {"TiO2", "Nb2O5"})
        self.assertEqual(cached["TiO2"]["fetched_at_utc"], "2026-08-16T00:00:00+00:00")

    def test_legacy_row_without_timestamp_loads_with_none(self):
        legacy = {"formula": "VO2", "data": [], "meta": {}}  # no fetched_at_utc, pre-v2 shape
        self.cache_path.write_text(json.dumps(legacy) + "\n")
        cached = foc._load_cache(self.cache_path)
        self.assertIsNone(cached["VO2"].get("fetched_at_utc"))

    def test_truncated_final_line_is_dropped_and_file_truncated(self):
        good = [_row("TiO2"), _row("Nb2O5")]
        good_text = "".join(json.dumps(r) + "\n" for r in good)
        truncated_tail = '{"formula": "VO2", "data": [{"name": "VO2"'  # no trailing newline
        self.cache_path.write_text(good_text + truncated_tail)

        with _capture_stderr() as stderr:
            cached = foc._load_cache(self.cache_path)

        self.assertEqual(set(cached), {"TiO2", "Nb2O5"})
        self.assertIn("truncated final line", stderr.getvalue())
        # The file on disk was repaired: re-loading gives the same result,
        # and a subsequent append produces a valid, parseable cache.
        remaining = self.cache_path.read_text()
        self.assertEqual(remaining, good_text)

    def test_midfile_corruption_raises(self):
        content = json.dumps(_row("TiO2")) + "\n" + "{not valid json\n" + json.dumps(_row("Nb2O5")) + "\n"
        self.cache_path.write_text(content)
        with self.assertRaises(foc.OqmdFetchError):
            foc._load_cache(self.cache_path)

    def test_final_line_corrupt_but_newline_terminated_raises(self):
        # The line IS fully written (trailing newline present) but its
        # content is simply bad JSON -- not a truncation, must fail loud.
        content = json.dumps(_row("TiO2")) + "\n" + "{not valid json}\n"
        self.cache_path.write_text(content)
        with self.assertRaises(foc.OqmdFetchError):
            foc._load_cache(self.cache_path)


class _capture_stderr:
    def __enter__(self):
        import io
        import sys

        self._old = sys.stderr
        self._buf = io.StringIO()
        sys.stderr = self._buf
        return self._buf

    def __exit__(self, *exc):
        import sys

        sys.stderr = self._old


class CacheFingerprintTests(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.cache_path = Path(self._tmp.name) / "cache.jsonl"
        self.fp = foc._current_fingerprint("deadbeef")

    def tearDown(self):
        self._tmp.cleanup()

    def test_fresh_cache_writes_fingerprint(self):
        foc.resolve_cache_fingerprint(self.cache_path, self.fp, trust_legacy_cache=False)
        meta_path = foc._cache_meta_path(self.cache_path)
        self.assertEqual(json.loads(meta_path.read_text()), self.fp)

    def test_matching_fingerprint_resumes_without_error(self):
        foc.resolve_cache_fingerprint(self.cache_path, self.fp, trust_legacy_cache=False)
        # Second run, same fingerprint -- must not raise.
        foc.resolve_cache_fingerprint(self.cache_path, self.fp, trust_legacy_cache=False)

    def test_mismatched_fingerprint_raises(self):
        foc.resolve_cache_fingerprint(self.cache_path, self.fp, trust_legacy_cache=False)
        other_fp = foc._current_fingerprint("different-population-hash")
        with self.assertRaises(foc.OqmdFetchError):
            foc.resolve_cache_fingerprint(self.cache_path, other_fp, trust_legacy_cache=False)

    def test_legacy_cache_without_meta_requires_trust_flag(self):
        self.cache_path.write_text(json.dumps(_row("TiO2")) + "\n")
        with self.assertRaises(foc.OqmdFetchError):
            foc.resolve_cache_fingerprint(self.cache_path, self.fp, trust_legacy_cache=False)
        # With the flag, it's adopted and a meta file is written so a
        # later run no longer needs the flag.
        foc.resolve_cache_fingerprint(self.cache_path, self.fp, trust_legacy_cache=True)
        foc.resolve_cache_fingerprint(self.cache_path, self.fp, trust_legacy_cache=False)


class RetrievalMetadataTests(unittest.TestCase):
    def test_all_known_timestamps(self):
        meta = foc.build_retrieval_metadata(
            ["2026-08-16T01:00:00+00:00", "2026-08-15T09:00:00+00:00"],
            resumed_from_cache_count=1,
            unknown_timestamp_count=0,
            completed_at_utc="2026-08-16T02:00:00+00:00",
        )
        self.assertEqual(meta["first_fetch_at_utc"], "2026-08-15T09:00:00+00:00")
        self.assertEqual(meta["last_fetch_at_utc"], "2026-08-16T01:00:00+00:00")
        self.assertTrue(meta["across_multiple_runs"])

    def test_all_unknown_timestamps(self):
        meta = foc.build_retrieval_metadata(
            [], resumed_from_cache_count=714, unknown_timestamp_count=714, completed_at_utc="2026-08-16T02:00:00+00:00"
        )
        self.assertIsNone(meta["first_fetch_at_utc"])
        self.assertIsNone(meta["last_fetch_at_utc"])
        self.assertEqual(meta["unknown_fetch_timestamp_count"], 714)
        self.assertTrue(meta["across_multiple_runs"])

    def test_single_run_no_resume(self):
        meta = foc.build_retrieval_metadata(
            ["2026-08-16T01:00:00+00:00"],
            resumed_from_cache_count=0,
            unknown_timestamp_count=0,
            completed_at_utc="2026-08-16T02:00:00+00:00",
        )
        self.assertFalse(meta["across_multiple_runs"])


class SchemaValidationTests(unittest.TestCase):
    def test_empty_first_response_does_not_lock_in_validated(self):
        empty_payload = {"data": [], "meta": {}}
        validated = foc.maybe_validate_schema(False, empty_payload, "TiO2")
        self.assertFalse(validated)

        bad_payload = {"data": [{"name": "TiO2", "entry_id": 1}], "meta": {}}  # missing natoms/volume/delta_e
        with self.assertRaises(foc.OqmdFetchError):
            foc.maybe_validate_schema(validated, bad_payload, "TiO2")

    def test_once_validated_stays_validated(self):
        good_payload = {
            "data": [{"name": "TiO2", "entry_id": 1, "natoms": 6, "volume": 30.0, "delta_e": -3.2}],
            "meta": {},
        }
        validated = foc.maybe_validate_schema(False, good_payload, "TiO2")
        self.assertTrue(validated)
        # A later malformed payload must not be re-checked once validated.
        bad_payload = {"data": [{"name": "VO2"}], "meta": {}}
        self.assertTrue(foc.maybe_validate_schema(validated, bad_payload, "VO2"))


class QueryCompositionRetryTests(unittest.TestCase):
    def setUp(self):
        self._sleep_patch = mock.patch.object(foc.time, "sleep", return_value=None)
        self.mock_sleep = self._sleep_patch.start()

    def tearDown(self):
        self._sleep_patch.stop()

    def test_transient_429_retries_then_raises(self):
        err = urllib.error.HTTPError("url", 429, "Too Many Requests", {}, None)
        with mock.patch.object(foc, "_urlopen", side_effect=err) as mocked:
            with self.assertRaises(foc.OqmdFetchError) as ctx:
                foc.query_composition("TiO2", timeout=1, retries=4, backoff_seconds=(0, 0, 0, 0))
            self.assertEqual(mocked.call_count, 4)
            self.assertIn("gave up after 4 attempts", str(ctx.exception))
            self.assertEqual(self.mock_sleep.call_count, 3)

    def test_non_transient_malformed_json_no_retry(self):
        resp = mock.MagicMock()
        resp.status = 200
        resp.read.return_value = b"not json"
        resp.__enter__.return_value = resp
        resp.__exit__.return_value = False
        with mock.patch.object(foc, "_urlopen", return_value=resp) as mocked:
            with self.assertRaises(foc.OqmdFetchError):
                foc.query_composition("TiO2", timeout=1, retries=4, backoff_seconds=(0, 0, 0, 0))
            self.assertEqual(mocked.call_count, 1)
            self.mock_sleep.assert_not_called()

    def test_non_transient_http_404_no_retry(self):
        err = urllib.error.HTTPError("url", 404, "Not Found", {}, None)
        with mock.patch.object(foc, "_urlopen", side_effect=err) as mocked:
            with self.assertRaises(foc.OqmdFetchError):
                foc.query_composition("TiO2", timeout=1, retries=4, backoff_seconds=(0, 0, 0, 0))
            self.assertEqual(mocked.call_count, 1)
            self.mock_sleep.assert_not_called()


if __name__ == "__main__":
    unittest.main()
