#!/usr/bin/env python3
"""Lightweight daily health check: does OQMD's REST API currently return
usable formation-energy data -- not just "does the site respond"? Used
by .github/workflows/oqmd-recovery-check.yml, which is a coverage-only
monitor -- it reports whether OQMD's data availability changed, and is
NOT a signal that any phase should resume. Per-species OQMD coverage
was never Phase 21B's real bottleneck (of 1285 OQMD-covered,
formula-parseable rows, only 347 actually balanced into a valid
reaction -- docs/phase21b_calibration_result.md); Phase 21B's real
reopening bar is a newly-qualified corpus with >=100 new independent
target pairs (docs/phase32_reaction_record_qualification.md).

This script only detects an availability change and reports it. It
never runs the real Phase 21B fetcher (benchmarks/fetch_oqmd_coverage.py),
never writes a coverage manifest, and never starts calibration --
availability detection and any real coverage measurement are
deliberately separate steps, gated on a fresh owner trigger for the
latter.

"Healthy" means: HTTP 200, valid JSON, the expected top-level keys
(data/meta) present, the first returned entry has every field the real
fetcher requires (name/entry_id/natoms/volume/delta_e), and at least
one returned entry has a non-null delta_e -- i.e. OQMD returns at least
one row Phase 21B could actually use, not merely "the server answered."

Exit code is 0 in both the healthy and unhealthy case -- an unhealthy
result is an expected, recurring outcome during the outage, not a
script failure. The caller reads GITHUB_OUTPUT's `healthy` value. A
non-zero exit means this check itself malfunctioned (a real bug here)
and should fail the workflow loudly rather than being silently folded
into "still down."
"""

import argparse
import json
import os
import ssl
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

try:
    import certifi

    _SSL_CONTEXT = ssl.create_default_context(cafile=certifi.where())
except ImportError:
    _SSL_CONTEXT = None

OQMD_BASE = "https://oqmd.org/oqmdapi/formationenergy"
# A common oxide already used elsewhere in this project's own fixtures
# (tests/fixtures/batio3_report.json) -- virtually guaranteed to have
# OQMD entries if the service is genuinely back, so a "healthy" result
# reflects OQMD's data being usable, not a query that happens to match
# nothing.
HEALTH_CHECK_COMPOSITION = "TiO2"
# Small on purpose (docs/thermodynamic_selectivity_calibration.md Sec 6's
# own "minimize OQMD load" discipline): just enough entries that "every
# returned entry has a null delta_e" isn't a false negative from bad luck
# on a single row.
HEALTH_CHECK_LIMIT = 3
QUERY_FIELDS = [
    "name",
    "entry_id",
    "natoms",
    "volume",
    "delta_e",
    "spacegroup",
    "stability",
    "duplicate_entry_id",
]
REQUIRED_ENTRY_FIELDS = {"name", "entry_id", "natoms", "volume", "delta_e"}
REQUIRED_RESPONSE_KEYS = {"data", "meta"}


def fetch(timeout):
    """One HTTP attempt. Returns (status_or_None, body_bytes_or_None,
    error_message_or_None). Never raises for network/HTTP-level
    failures -- those are expected "still down" outcomes for this
    script's purpose, not bugs."""
    params = urllib.parse.urlencode(
        {
            "composition": HEALTH_CHECK_COMPOSITION,
            "limit": HEALTH_CHECK_LIMIT,
            "fields": ",".join(QUERY_FIELDS),
        }
    )
    url = f"{OQMD_BASE}?{params}"
    req = urllib.request.Request(url, headers={"User-Agent": "gugen-oqmd-recovery-check/1.0"})
    try:
        with urllib.request.urlopen(req, context=_SSL_CONTEXT, timeout=timeout) as resp:
            return resp.status, resp.read(), None
    except urllib.error.HTTPError as e:
        return e.code, None, None
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        return None, None, f"network error: {e}"


def _evaluate_response(status, body):
    if status != 200:
        return False, f"HTTP {status}"
    try:
        payload = json.loads(body)
    except json.JSONDecodeError:
        return False, "response body is not valid JSON (HTML error page or similar)"
    if not isinstance(payload, dict):
        return False, "response JSON is not an object"
    missing_top = REQUIRED_RESPONSE_KEYS - payload.keys()
    if missing_top:
        return False, f"response missing top-level keys {sorted(missing_top)}"
    data = payload["data"]
    if not isinstance(data, list) or not data:
        return False, "response 'data' is empty -- OQMD answered but returned no entries for a common compound"
    missing_fields = REQUIRED_ENTRY_FIELDS - data[0].keys()
    if missing_fields:
        return False, f"first entry missing required fields {sorted(missing_fields)}"
    if not any(e.get("delta_e") is not None for e in data):
        return False, "every returned entry has a null delta_e -- no usable formation-energy value"
    n = len(data)
    return True, f"OQMD returned {n} usable entr{'y' if n == 1 else 'ies'} for {HEALTH_CHECK_COMPOSITION!r}"


def check_health(fetch_fn=fetch, timeout=15, attempts=3, backoff_seconds=(5, 15)):
    """Returns (healthy: bool, reason: str). Retries on network error or
    a non-200 status (a 502 today could be a transient blip on a
    genuinely-recovering service) using a short fixed backoff; a 200
    response is evaluated once and not retried, since a schema/content
    problem in a 200 response won't change on a re-request. Bounded to
    `attempts` total requests (default 3) regardless of outcome, per
    this project's "don't hammer OQMD" discipline."""
    last_reason = "not attempted"
    for attempt in range(attempts):
        status, body, err = fetch_fn(timeout)
        if err is not None:
            last_reason = err
        elif status != 200:
            last_reason = f"HTTP {status}"
        else:
            return _evaluate_response(status, body)
        if attempt < attempts - 1:
            time.sleep(backoff_seconds[min(attempt, len(backoff_seconds) - 1)])
    return False, f"all {attempts} attempts failed: {last_reason}"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout", type=float, default=15)
    parser.add_argument("--attempts", type=int, default=3)
    args = parser.parse_args()

    healthy, reason = check_health(timeout=args.timeout, attempts=args.attempts)
    print(f"healthy={healthy}: {reason}", file=sys.stderr)

    github_output = os.environ.get("GITHUB_OUTPUT")
    if github_output:
        with open(github_output, "a") as f:
            f.write(f"healthy={'true' if healthy else 'false'}\n")
            f.write(f"reason={reason}\n")


if __name__ == "__main__":
    main()
