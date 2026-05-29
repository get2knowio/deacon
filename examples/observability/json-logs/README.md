# JSON Logs (observability)

This example shows how to enable structured JSON logging and parse it for
fields such as `message`, `target`, and `span` data.

The Deacon CLI emits structured logs to **stderr** while keeping **stdout**
reserved for command results (e.g. JSON configuration output). This separation
lets you parse logs and results independently.

## Usage

```bash
deacon --log-format json read-configuration > config.json 2> logs.jsonl
```

- `config.json` (stdout) is a single JSON document — the command result.
- `logs.jsonl` (stderr) carries the structured logs, one JSON object per line.

Add `--log-level debug` to increase log verbosity; stdout stays a clean,
single JSON document regardless of how much is logged.

## Scenarios

`exec.sh` verifies the Output Streams Contract:

1. `--log-format json` → stdout is one valid JSON document.
2. `--log-format json --log-level debug` → stderr is newline-delimited JSON log
   objects (each with a `timestamp`), and stdout stays a clean JSON document
   (logs never leak onto stdout).
3. Default text mode → the result is on stdout with no JSON-log leakage.
