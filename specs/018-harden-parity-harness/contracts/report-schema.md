# Contract: Parity Report Schemas

**Feature**: 018-harden-parity-harness

Two JSON documents. Writers use atomic temp-file + rename. Consumers:
`parity-report` aggregator, CI artifact viewers, humans diagnosing failures.
Schemas are normative; unknown fields are rejected on read.

## ReportFragment — `target/parity/report/<binary>.json`

```json
{
  "binary": "parity_corpus_tier1",
  "oracle": {
    "version": "0.87.0",
    "path": "/usr/local/bin/devcontainer",
    "source": "path-lookup"
  },
  "mode": "live",
  "started": "2026-07-19T04:12:03Z",
  "finished": "2026-07-19T04:14:41Z",
  "cases": [
    {
      "case": "extends-child",
      "outcome": "pass",
      "waivers_applied": [],
      "raw": {
        "deacon_stdout": "raw/parity_corpus_tier1/extends-child/deacon.stdout",
        "deacon_stderr": "raw/parity_corpus_tier1/extends-child/deacon.stderr",
        "oracle_stdout": "raw/parity_corpus_tier1/extends-child/oracle.stdout",
        "oracle_stderr": "raw/parity_corpus_tier1/extends-child/oracle.stderr"
      }
    },
    {
      "case": "ports-mixed",
      "outcome": "fail",
      "cause": "divergence",
      "diff_summary": "value mismatch at forwardPorts[1]: ours=8080 ref=\"8080\"",
      "waivers_applied": [],
      "raw": { "…": "…" }
    },
    {
      "case": "errors/extends-missing",
      "outcome": "pass-waived",
      "waivers_applied": ["errors/extends-missing"],
      "raw": { "…": "…" }
    }
  ],
  "omitted": []
}
```

Field rules:

- `mode`: only `"live"` is produced by this feature; the field is mandatory so
  any future replay mode is visibly distinct (FR-017).
- `outcome` ∈ `pass` | `pass-waived` | `fail`.
- `cause` (required iff `fail`) ∈ `divergence` | `oracle-failure` |
  `oracle-timeout` | `malformed-output` | `normalization` | `fixture-missing` |
  `docker-missing`.
- `pass-waived` requires non-empty `waivers_applied`, each id resolvable to an
  active waiver record (FR-010).
- `raw` paths are relative to the report dir and MUST exist (FR-020); the writer
  fails the test if it cannot produce them (FR-018).
- `omitted` entries require a `reason`; the aggregator treats unexplained
  omission as failure (FR-016, SC-003).

## AggregatedReport — `target/parity/parity-report.json`

```json
{
  "oracle": {
    "pin": { "package": "@devcontainers/cli", "version": "0.87.0" },
    "verified_version": "0.87.0",
    "path": "/usr/local/bin/devcontainer"
  },
  "binaries": [
    { "binary": "parity_read_configuration", "cases": 2, "passed": 2, "waived": 0, "failed": 0, "omitted": 0 },
    { "binary": "parity_corpus_tier1", "cases": 23, "passed": 22, "waived": 1, "failed": 0, "omitted": 0 }
  ],
  "missing_fragments": [],
  "stale_waivers": [],
  "totals": { "cases": 25, "passed": 24, "waived": 1, "failed": 0, "omitted": 0 }
}
```

Aggregator exit is nonzero unless ALL hold:

1. `missing_fragments == []` — a fragment exists for every registry
   `live_binaries` entry (proves execution, FR-022);
2. every fragment's `oracle.version` equals the pin and all fragments agree on
   `path`;
3. `totals.failed == 0` and every `omitted` has a reason;
4. `stale_waivers == []` (FR-011);
5. every corpus met its registry `min_cases` (FR-024);
6. the report file itself was written successfully (FR-018).
