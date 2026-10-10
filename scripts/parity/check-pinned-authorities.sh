#!/usr/bin/env bash
# Has either AUTHORITY this project measures against moved?
#
# deacon's correctness claims rest on two pins: the spec commit under `parity/spec/`
# and the reference CLI version in `parity/oracle.json`. Both are asserted at RUN time
# — `parity.yml` fails if the installed oracle is not the pinned one — and that check
# is for reproducibility. It is structurally blind to the question this script asks:
# has UPSTREAM moved past the pin?
#
# That blindness had a cost. #745 pinned buildx below 0.37 in the oracle lane,
# accepting a real loss of coverage, because the oracle at 0.87.0 cannot run a Compose
# `up` on 0.37. 0.89.0 was available at the time and nobody knew to look. It turned
# out not to fix it — MEASURED, not assumed — but that is luck, not process: had it
# been fixed two releases earlier, the project would have carried a self-inflicted
# blind spot indefinitely. For a suite whose discipline is "measure, don't assume",
# the two authorities the measurements are made AGAINST were the one thing nobody was
# measuring.
#
# REPORTS, NEVER GATES. A pin moves on a human's decision: bumping the oracle
# re-baselines every `live-differential` case, so it is a deliberate exercise
# (bump, run the nightly, diff the diverging set, adjudicate each change), not
# something a cron job should do. Exit 10 means "drift found", which the workflow
# turns into a tracking issue; it must never become a required check.
#
# WHY sha256 PER DOCUMENT rather than "is the commit still HEAD": because the answer
# has to be worth reading. Between the pin and upstream HEAD at the time of writing,
# exactly one of eighteen documents changed, and it was `supporting-tools.md` — a
# listing of editors that support devcontainers, not normative text. A check that
# fired on any new commit would have cried wolf on that and been ignored by the third
# firing. The manifest already records a sha256 per document, so drift can be reported
# exactly: which document, and whether it is one deacon's surface depends on.
set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
oracle_pin_file="${root}/parity/oracle.json"
spec_root="${root}/parity/spec"

DRIFT=0
SOFT_FAIL=0

say() { printf '%s\n' "$*"; }

# ---------------------------------------------------------------------------
# 1. The reference CLI
# ---------------------------------------------------------------------------
say "## Reference CLI"
say ""

pinned_oracle="$(python3 -c "import json;print(json.load(open('${oracle_pin_file}'))['version'])" 2>/dev/null || true)"
pinned_pkg="$(python3 -c "import json;print(json.load(open('${oracle_pin_file}'))['package'])" 2>/dev/null || true)"

if [ -z "${pinned_oracle}" ] || [ -z "${pinned_pkg}" ]; then
  say "- **Could not read the pin** from \`parity/oracle.json\`. This is a repository"
  say "  fault, not drift — the file is the pin's authority and must parse."
  exit 1
fi

latest_oracle="$(npm view "${pinned_pkg}" version 2>/dev/null | tail -1 | tr -d '[:space:]' || true)"

if [ -z "${latest_oracle}" ]; then
  # An unreachable registry is NOT drift. Saying so explicitly matters: a check that
  # reports "no drift" when it could not look is worse than one that reports nothing.
  say "- ⚠️ **Could not reach npm** to resolve \`${pinned_pkg}\`. No conclusion drawn."
  SOFT_FAIL=1
elif [ "${pinned_oracle}" = "${latest_oracle}" ]; then
  say "- Pinned \`${pinned_pkg}@${pinned_oracle}\` **is** the latest. No drift."
else
  say "- **DRIFT**: pinned \`${pinned_pkg}@${pinned_oracle}\`, latest is **${latest_oracle}**."
  say "- Compare: <https://github.com/devcontainers/cli/compare/v${pinned_oracle}...v${latest_oracle}>"
  say ""
  say "  A bump re-baselines every \`live-differential\` case. The procedure is to bump the"
  say "  pin, run the nightly, diff the diverging set against the current one, and adjudicate"
  say "  each change — not to take the new version because it is newer."
  DRIFT=1
fi

# ---------------------------------------------------------------------------
# 2. The spec
# ---------------------------------------------------------------------------
say ""
say "## Spec"
say ""

# The on-disk directory name IS the pin; there is exactly one.
spec_pin="$(find "${spec_root}" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' 2>/dev/null)"
if [ "$(printf '%s\n' "${spec_pin}" | grep -c .)" != "1" ]; then
  say "- **Expected exactly one commit directory under \`parity/spec/\`**, found:"
  printf '  - %s\n' ${spec_pin:-<none>}
  say "  The pin is ambiguous; that is a repository fault, not drift."
  exit 1
fi

manifest="${spec_root}/${spec_pin}/manifest.json"
if [ ! -f "${manifest}" ]; then
  say "- **No manifest** at \`parity/spec/${spec_pin}/manifest.json\`; cannot compare per document."
  exit 1
fi

upstream_head="$(curl -fsSL -H 'Accept: application/vnd.github+json' \
  'https://api.github.com/repos/devcontainers/spec/commits?per_page=1' 2>/dev/null \
  | python3 -c "import json,sys; print(json.load(sys.stdin)[0]['sha'])" 2>/dev/null || true)"

if [ -z "${upstream_head}" ]; then
  say "- ⚠️ **Could not reach the GitHub API** for \`devcontainers/spec\`. No conclusion drawn."
  SOFT_FAIL=1
elif [ "${upstream_head:0:8}" = "${spec_pin}" ]; then
  say "- Pinned commit \`${spec_pin}\` **is** upstream HEAD. No drift."
else
  say "- Pinned commit \`${spec_pin}\`; upstream HEAD is \`${upstream_head:0:8}\`."
  say ""
  say "  A new commit is NOT by itself drift that matters — what matters is whether a"
  say "  document deacon cites changed. Comparing recorded \`sha256\` against HEAD:"
  say ""

  changed=0; unchanged=0; unreachable=0
  while IFS=$'\t' read -r file scope recorded; do
    [ -z "${file}" ] && continue
    url="https://raw.githubusercontent.com/devcontainers/spec/${upstream_head}/docs/specs/${file}"
    actual="$(curl -fsSL "${url}" 2>/dev/null | sha256sum 2>/dev/null | cut -d' ' -f1 || true)"
    if [ -z "${actual}" ]; then
      say "  - ⚠️ \`${file}\` (${scope}) — could not fetch; no conclusion"
      unreachable=$((unreachable + 1))
      SOFT_FAIL=1
    elif [ "${actual}" = "${recorded}" ]; then
      unchanged=$((unchanged + 1))
    else
      say "  - **CHANGED**: \`${file}\` — scope \`${scope}\`"
      changed=$((changed + 1))
    fi
  done < <(python3 -c "
import json
m = json.load(open('${manifest}'))
for d in m['documents']:
    print('\t'.join((d['file'], d.get('scope', '?'), d['sha256'])))
")

  say ""
  say "  ${changed} changed, ${unchanged} unchanged, ${unreachable} unreachable (of $((changed + unchanged + unreachable)))."
  if [ "${changed}" -gt 0 ]; then
    say ""
    say "  A \`consumer\`-scope change is the one that can invalidate a ledger row or a case."
    say "  An \`authoring\`-scope change is out of deacon's declared scope (constitution III:"
    say "  feature/template AUTHORING is permanently out of scope) and needs no action beyond"
    say "  re-pinning when convenient. \`supporting-tools.md\` carries \`consumer\` scope but is"
    say "  a listing of editors, not normative text — it drifts without meaning anything."
    DRIFT=1
  else
    say ""
    say "  **No document deacon measures against has changed.** The pin is sound; re-pinning"
    say "  would be bookkeeping, not correctness."
  fi
fi

say ""
if [ "${SOFT_FAIL}" = "1" ]; then
  say "_One or more lookups failed; treat the above as partial._"
fi

if [ "${DRIFT}" = "1" ]; then
  say "**Verdict: drift found.** Reported, not gated — see the notes above for what each kind costs."
  exit 10
fi

say "**Verdict: both pins current.**"
exit 0
