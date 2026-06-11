#!/usr/bin/env python3
"""Tier-1 differential parity driver: read-configuration, deacon vs reference CLI.

Runs both CLIs over every corpus config, normalizes the JSON (unwrap the
reference's {configuration} wrapper, drop volatile/default noise), and prints a
ranked divergence report. High signal first: ref-only keys (deacon dropping
data) and value mismatches; deacon-only keys (mostly default noise) last.

Usage: python3 fixtures/parity-corpus/run_tier1.py [deacon_bin] [corpus_dir]
"""
import json
import subprocess
import sys
from pathlib import Path

DEACON = sys.argv[1] if len(sys.argv) > 1 else "/workspaces/deacon/target/debug/deacon"
CORPUS = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).parent

# Keys the reference adds that deacon legitimately won't (or vice versa) — pure noise.
DROP_KEYS = {"configFilePath"}


def run(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True)
    return p.returncode, p.stdout, p.stderr


def deacon_read(ws):
    return run([DEACON, "read-configuration", "--workspace-folder", str(ws)])


def ref_read(ws):
    return run(["devcontainer", "read-configuration", "--workspace-folder", str(ws)])


def prune(v):
    """Recursively drop nulls, empty arrays/objects/strings, and DROP_KEYS."""
    if isinstance(v, dict):
        out = {}
        for k, val in v.items():
            if k in DROP_KEYS:
                continue
            pv = prune(val)
            if pv is None:
                continue
            if isinstance(pv, (dict, list, str)) and len(pv) == 0:
                continue
            out[k] = pv
        return out
    if isinstance(v, list):
        return [prune(x) for x in v]
    return v


def normalize(raw):
    obj = json.loads(raw)
    if isinstance(obj, dict) and "configuration" in obj and isinstance(obj["configuration"], dict):
        obj = obj["configuration"]
    return prune(obj)


def diff(d, r, path=""):
    """Return list of (kind, path, deacon_val, ref_val)."""
    out = []
    if isinstance(d, dict) and isinstance(r, dict):
        for k in sorted(set(d) | set(r)):
            p = f"{path}.{k}" if path else k
            if k in d and k not in r:
                out.append(("deacon-only", p, d[k], None))
            elif k in r and k not in d:
                out.append(("ref-only", p, None, r[k]))
            else:
                out.extend(diff(d[k], r[k], p))
    elif isinstance(d, list) and isinstance(r, list):
        if d != r:
            out.append(("value", path, d, r))
    else:
        if d != r:
            out.append(("value", path, d, r))
    return out


def main():
    configs = sorted(p for p in CORPUS.iterdir() if (p / ".devcontainer").is_dir())
    rank = {"ref-only": 0, "value": 1, "deacon-only": 2}
    totals = {"ref-only": 0, "value": 0, "deacon-only": 0}
    crashes = []

    for ws in configs:
        name = ws.name
        dc, dout, derr = deacon_read(ws)
        rc, rout, rerr = ref_read(ws)
        print(f"\n=== {name} ===")
        if dc != 0:
            print(f"  ⚠️  DEACON crashed (exit {dc}): {derr.strip()[:400]}")
            crashes.append((name, "deacon", derr.strip()[:400]))
            continue
        if rc != 0:
            print(f"  ⚠️  REFERENCE crashed (exit {rc}): {rerr.strip()[:300]}")
            crashes.append((name, "reference", rerr.strip()[:300]))
            continue
        try:
            dn = normalize(dout)
            rn = normalize(rout)
        except Exception as e:
            print(f"  ⚠️  normalize failed: {e}")
            continue
        ds = diff(dn, rn)
        if not ds:
            print("  ✅ identical (after normalization)")
            continue
        ds.sort(key=lambda x: rank[x[0]])
        for kind, p, dv, rv in ds:
            totals[kind] += 1
            if kind == "ref-only":
                print(f"  ❗ ref-only   {p} = {json.dumps(rv)[:200]}  (deacon DROPS this)")
            elif kind == "value":
                print(f"  ⚡ mismatch   {p}\n        deacon={json.dumps(dv)[:200]}\n        ref   ={json.dumps(rv)[:200]}")
            else:
                print(f"  · deacon-only {p} = {json.dumps(dv)[:120]}")

    print("\n========== SUMMARY ==========")
    print(f"configs: {len(configs)}  crashes: {len(crashes)}")
    print(f"ref-only (deacon drops): {totals['ref-only']}")
    print(f"value mismatches:        {totals['value']}")
    print(f"deacon-only (noise?):    {totals['deacon-only']}")
    for n, who, msg in crashes:
        print(f"  CRASH {n} [{who}]: {msg[:160]}")


if __name__ == "__main__":
    main()
