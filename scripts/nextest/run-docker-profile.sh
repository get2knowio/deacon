#!/usr/bin/env bash
set -euo pipefail
# Helper to run cargo nextest docker profile and handle no-tests as informational
./scripts/nextest/assert-installed.sh
mkdir -p artifacts/nextest

echo "Running nextest docker profile..."
start_time=$(date +%s)

out_file=artifacts/nextest/docker-run-out.txt
rm -f "${out_file}" || true

# Pass any provided THREAD_ARGS as arguments (they might be '-j N')
# We accept all args and forward them to cargo nextest
if cargo nextest run --profile docker "$@" > "${out_file}" 2>&1; then
  rc=0
else
  rc=$?
fi
out_and_err=$(cat "${out_file}" || true)
if [[ ${rc:-0} -eq 0 ]]; then
  end_time=$(date +%s); duration=$((end_time - start_time)); timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
  echo "${out_and_err}"
  echo "{\"profile\":\"docker\",\"duration_seconds\":${duration},\"timestamp_utc\":\"${timestamp}\",\"exit_code\":0}" > artifacts/nextest/docker-timing.json
  echo "✓ Docker profile tests passed in ${duration}s. Timing data: artifacts/nextest/docker-timing.json"
else
  if echo "${out_and_err}" | grep -qi "no tests to run"; then
    echo "No docker-specific tests found; skipping."
    exit 0
  fi
  echo "${out_and_err}"
  end_time=$(date +%s); duration=$((end_time - start_time)); timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
  echo "{\"profile\":\"docker\",\"duration_seconds\":${duration},\"timestamp_utc\":\"${timestamp}\",\"exit_code\":${rc}}" > artifacts/nextest/docker-timing.json
  echo "✗ Docker profile tests failed after ${duration}s. Timing data: artifacts/nextest/docker-timing.json"
  exit ${rc}
fi
