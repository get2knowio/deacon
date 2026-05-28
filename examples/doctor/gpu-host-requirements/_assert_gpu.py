"""Validate the resolved `hostRequirements.gpu` shape for the gpu example.

Usage: _assert_gpu.py <expected_type> <json_file>
expected_type: one of `bool`, `string`, `object`.
"""

import json
import sys


def main() -> None:
    expected, path = sys.argv[1], sys.argv[2]
    with open(path) as f:
        doc = json.load(f)
    gpu = doc.get("configuration", {}).get("hostRequirements", {}).get("gpu")
    if expected == "bool":
        assert gpu is True, gpu
    elif expected == "string":
        assert gpu == "optional", gpu
    elif expected == "object":
        assert isinstance(gpu, dict), gpu
        assert gpu.get("cores") == 2 and gpu.get("memory") == "8gb", gpu
    else:
        raise SystemExit(f"unknown expected type: {expected}")
    print(f"  ok: gpu parsed as {expected}: {json.dumps(gpu)}")


if __name__ == "__main__":
    main()
