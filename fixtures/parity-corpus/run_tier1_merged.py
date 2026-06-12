#!/usr/bin/env python3
"""Tier-1b differential parity driver: the `mergedConfiguration` block.

Like `run_tier1.py`, but adds `--include-merged-configuration` and diffs the
`mergedConfiguration` object (deacon vs reference CLI) after the same null/empty
normalization. Surfaces image-metadata-merge divergences (collected lifecycle
arrays, hostRequirements byte normalization, init/privileged defaults,
customizations ordering, feature-vs-image env handling).

Note: `mergedConfiguration` folds the image's `devcontainer.metadata` label, so
results depend on which base images are pulled locally — deacon reads metadata
best-effort (local-only) while the reference pulls. Configs whose base image is
absent will show image-metadata divergences that vanish once the image is present
(see REPORT.md "Verified non-bugs").

Usage: python3 fixtures/parity-corpus/run_tier1_merged.py [deacon_bin] [corpus_dir]
"""
import json
import subprocess
import sys
from pathlib import Path

DEACON = sys.argv[1] if len(sys.argv) > 1 else "/workspaces/deacon/target/debug/deacon"
CORPUS = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).parent

DROP_KEYS = {"configFilePath"}


def run(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True)
    return p.returncode, p.stdout, p.stderr


def prune(v):
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


def merged(raw):
    obj = json.loads(raw)
    return prune(obj.get("mergedConfiguration", {}))


def diff(d, r, path=""):
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

    for ws in configs:
        dc, dout, derr = run(
            [DEACON, "read-configuration", "--workspace-folder", str(ws),
             "--include-merged-configuration"]
        )
        rc, rout, rerr = run(
            ["devcontainer", "read-configuration", "--workspace-folder", str(ws),
             "--include-merged-configuration"]
        )
        print(f"\n=== {ws.name} ===")
        if dc != 0:
            print(f"  ⚠️  DEACON crashed (exit {dc}): {derr.strip()[:300]}")
            continue
        if rc != 0:
            print(f"  ⚠️  REFERENCE crashed (exit {rc}): {rerr.strip()[:300]}")
            continue
        try:
            dn, rn = merged(dout), merged(rout)
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
                print(f"  ❗ ref-only   {p} = {json.dumps(rv)[:160]}  (deacon DROPS this)")
            elif kind == "value":
                print(f"  ⚡ mismatch   {p}\n        deacon={json.dumps(dv)[:160]}\n        ref   ={json.dumps(rv)[:160]}")
            else:
                print(f"  · deacon-only {p} = {json.dumps(dv)[:120]}")

    print("\n========== SUMMARY ==========")
    print(f"configs: {len(configs)}")
    print(f"ref-only (deacon drops): {totals['ref-only']}")
    print(f"value mismatches:        {totals['value']}")
    print(f"deacon-only (noise?):    {totals['deacon-only']}")
    print("(image-metadata divergences depend on locally-pulled images — see REPORT.md)")


if __name__ == "__main__":
    main()
