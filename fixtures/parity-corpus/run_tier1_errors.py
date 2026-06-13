#!/usr/bin/env python3
"""Tier-1c differential parity driver: the ERROR path, deacon vs reference CLI.

Every fixture in `errors/<name>/` is an *invalid* or edge-case devcontainer
config. Where the valid-config tiers (`run_tier1.py`, `run_tier1_merged.py`) diff
*successful* output, this tier asks the question those tiers can't:

    Do the two CLIs AGREE on whether this input is an error?

Exact error wording is expected to differ between a Rust CLI and a Node CLI, so
we deliberately do NOT diff messages. We diff the accept/reject *decision*
(exit-code class) and, when both accept, the resolved configuration value (after
the same null/default pruning `run_tier1.py` uses).

What this tier already surfaced (see errors/README.md): deacon validates
**eagerly and strictly** at `read-configuration` (typed deserialization + full
`extends` resolution), while the reference is a **lenient parse-and-echo** that
recovers from malformed JSON and does not resolve `extends` until up/build time.
Those are characterized, defensible divergences (deacon's constitution mandates
fail-fast / no silent fallbacks) — encoded as `deacon-stricter` so the corpus
goes red only if EITHER CLI's behavior *changes*.

Each fixture carries an `errors/<name>/expect.json`:

    {
      "description": "...",                 # human triage note
      "expect": "both-reject" | "both-accept" | "deacon-stricter",
      "config": "<rel/path>",               # optional explicit --config
      "signal": ["substr", ...]             # optional stderr substrings (info only)
    }

`expect` semantics (PASS = reality matches the encoded expectation):
  - both-reject     : both CLIs reject (exit != 0). True error-parity agreement.
  - both-accept     : both accept AND, after pruning, emit the same resolved
                      configuration. Catches silent value divergence on inputs
                      both tolerate (duplicate keys, untyped passthrough).
  - deacon-stricter : deacon rejects, reference accepts. A characterized
                      eager/strict-vs-lazy/lenient divergence. PASS while that
                      exact pattern holds; flagged if it flips either way.

Anything not matching its expectation is a DIVERGENCE (the signal to triage).
Exit status: 0 if every fixture PASSES, else 1 (CI-gateable).

Usage: python3 fixtures/parity-corpus/run_tier1_errors.py [deacon_bin] [corpus_dir]
"""
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_tier1 import normalize  # reuse {configuration}-unwrap + null/default prune

DEACON = sys.argv[1] if len(sys.argv) > 1 else "/workspaces/deacon/target/debug/deacon"
CORPUS = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).parent
ERRORS = CORPUS / "errors"

SNIP = 240  # stderr snippet length for triage output


def run(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True)
    return p.returncode, p.stdout, p.stderr


def build_command(cli, workspace, config_rel):
    cmd = [cli, "read-configuration", "--workspace-folder", str(workspace)]
    if config_rel:
        cmd.extend(["--config", str(workspace / config_rel)])
    return cmd


def cases():
    if not ERRORS.is_dir():
        return []
    out = []
    for d in sorted(ERRORS.iterdir()):
        spec = d / "expect.json"
        if d.is_dir() and spec.is_file():
            out.append((d.name, d, json.loads(spec.read_text())))
    return out


def values_agree(dout, rout):
    """True if the two CLIs' resolved configs match after pruning noise."""
    try:
        return normalize(dout) == normalize(rout)
    except Exception:
        return dout.strip() == rout.strip()


def check_signals(meta, derr, rerr):
    notes = []
    for s in meta.get("signal") or []:
        d_has, r_has = s.lower() in derr.lower(), s.lower() in rerr.lower()
        if not (d_has and r_has):
            notes.append(f"signal {s!r}: deacon={'y' if d_has else 'n'} ref={'y' if r_has else 'n'}")
    return notes


def evaluate(expect, d_accept, r_accept, dout, rout):
    """Return (is_pass, reason). reason is None on PASS."""
    if expect == "both-reject":
        if not d_accept and not r_accept:
            return True, None
        return False, f"expected both to reject (deacon {'accept' if d_accept else 'reject'}, ref {'accept' if r_accept else 'reject'})"
    if expect == "both-accept":
        if not (d_accept and r_accept):
            return False, f"expected both to accept (deacon {'accept' if d_accept else 'reject'}, ref {'accept' if r_accept else 'reject'})"
        if not values_agree(dout, rout):
            return False, "both accept but resolved configuration differs after pruning"
        return True, None
    if expect == "deacon-stricter":
        if not d_accept and r_accept:
            return True, None
        return False, f"expected deacon-reject / ref-accept, got deacon {'accept' if d_accept else 'reject'} / ref {'accept' if r_accept else 'reject'}"
    return False, f"unknown expect value: {expect!r}"


def main():
    found = cases()
    if not found:
        print(f"no error fixtures found under {ERRORS}")
        return 0

    passes = 0
    divergences = []

    for name, d, meta in found:
        config_rel = meta.get("config", "")
        expect = meta.get("expect", "both-reject")

        dc, dout, derr = run(build_command(DEACON, d, config_rel))
        rc, rout, rerr = run(build_command("devcontainer", d, config_rel))
        d_accept, r_accept = (dc == 0), (rc == 0)

        print(f"\n=== {name} ===")
        print(f"  {meta.get('description','').strip()}")
        print(f"  expect={expect}  deacon exit={dc} ({'accept' if d_accept else 'reject'})"
              f"  ref exit={rc} ({'accept' if r_accept else 'reject'})")

        is_pass, reason = evaluate(expect, d_accept, r_accept, dout, rout)

        if not d_accept:
            print(f"  deacon stderr: {derr.strip()[:SNIP]}")
        if not r_accept:
            print(f"  ref    stderr: {rerr.strip()[:SNIP]}")
        for n in check_signals(meta, derr, rerr):
            print(f"  ⚠️  {n}")

        if is_pass:
            passes += 1
            print("  ✅ parity as expected")
        else:
            divergences.append((name, reason))
            print(f"  ❗ DIVERGENCE: {reason}")

    print("\n========== SUMMARY ==========")
    print(f"fixtures: {len(found)}  as-expected: {passes}  divergences: {len(divergences)}")
    for n, r in divergences:
        print(f"  ❗ {n}: {r}")
    return 1 if divergences else 0


if __name__ == "__main__":
    sys.exit(main())
