#!/usr/bin/env python3
"""Self-check for check_oqmd_recovery.py's health-evaluation logic.
Run: python3 .github/scripts/test_check_oqmd_recovery.py

Every scenario is driven through fake `fetch_fn` callables so no real
HTTP request is made and no real GitHub Issue is created -- this
exercises the same 8-scenario matrix the owner asked for before the
workflow is trusted to run unattended against a real (recently
outage-prone) external service.
"""

import json
import unittest

from check_oqmd_recovery import check_health

HEALTHY_BODY = json.dumps(
    {
        "meta": {"query": {}},
        "data": [
            {"name": "TiO2", "entry_id": 1, "natoms": 6, "volume": 31.0, "delta_e": -3.4},
            {"name": "TiO2", "entry_id": 2, "natoms": 6, "volume": 30.5, "delta_e": None},
        ],
    }
).encode()

NULL_ENERGY_ONLY_BODY = json.dumps(
    {
        "meta": {},
        "data": [
            {"name": "TiO2", "entry_id": 1, "natoms": 6, "volume": 31.0, "delta_e": None},
        ],
    }
).encode()

MISSING_FIELD_BODY = json.dumps(
    {
        "meta": {},
        "data": [{"name": "TiO2", "entry_id": 1}],  # no natoms/volume/delta_e
    }
).encode()

MISSING_TOP_KEYS_BODY = json.dumps({"results": []}).encode()

HTML_ERROR_PAGE = b"<html><body><h1>502 Bad Gateway</h1></body></html>"

TRUNCATED_JSON = b'{"data": [{"name": "TiO2"'


def _once(status, body, err=None):
    """A fetch_fn stub that always returns the same result."""
    return lambda timeout: (status, body, err)


def _sequence(*results):
    """A fetch_fn stub that returns each result in turn (for retry tests)."""
    it = iter(results)
    return lambda timeout: next(it)


class HealthCheckScenarios(unittest.TestCase):
    """The matrix from the owner's release-review instructions."""

    def test_http_502_is_unhealthy_no_matter_the_attempt(self):
        healthy, reason = check_health(_once(502, None), attempts=3, backoff_seconds=(0, 0))
        self.assertFalse(healthy)
        self.assertIn("502", reason)

    def test_network_error_is_unhealthy(self):
        healthy, reason = check_health(_once(None, None, "network error: timed out"), attempts=3, backoff_seconds=(0, 0))
        self.assertFalse(healthy)
        self.assertIn("network error", reason)

    def test_html_response_is_unhealthy(self):
        healthy, reason = check_health(_once(200, HTML_ERROR_PAGE), attempts=1)
        self.assertFalse(healthy)
        self.assertIn("not valid JSON", reason)

    def test_malformed_json_is_unhealthy(self):
        healthy, reason = check_health(_once(200, TRUNCATED_JSON), attempts=1)
        self.assertFalse(healthy)
        self.assertIn("not valid JSON", reason)

    def test_missing_top_level_keys_is_unhealthy(self):
        healthy, reason = check_health(_once(200, MISSING_TOP_KEYS_BODY), attempts=1)
        self.assertFalse(healthy)
        self.assertIn("top-level keys", reason)

    def test_missing_required_entry_field_is_unhealthy(self):
        healthy, reason = check_health(_once(200, MISSING_FIELD_BODY), attempts=1)
        self.assertFalse(healthy)
        self.assertIn("missing required fields", reason)

    def test_all_null_delta_e_is_unhealthy(self):
        healthy, reason = check_health(_once(200, NULL_ENERGY_ONLY_BODY), attempts=1)
        self.assertFalse(healthy)
        self.assertIn("null delta_e", reason)

    def test_valid_healthy_response_is_healthy(self):
        healthy, reason = check_health(_once(200, HEALTHY_BODY), attempts=1)
        self.assertTrue(healthy)
        self.assertIn("usable entr", reason)

    def test_recovers_after_retryable_failures(self):
        # attempt 1: network error, attempt 2: HTTP 502, attempt 3: healthy.
        fetch_fn = _sequence((None, None, "network error: reset"), (502, None, None), (200, HEALTHY_BODY, None))
        healthy, reason = check_health(fetch_fn, attempts=3, backoff_seconds=(0, 0))
        self.assertTrue(healthy)

    def test_a_bug_in_fetch_fn_is_not_swallowed(self):
        def broken_fetch(timeout):
            raise ValueError("boom -- a real bug in this checker, not an OQMD condition")

        with self.assertRaises(ValueError):
            check_health(broken_fetch, attempts=3, backoff_seconds=(0, 0))


if __name__ == "__main__":
    unittest.main()
