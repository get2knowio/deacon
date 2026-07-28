# Contract: Execution Manifest

The receipt that lets a hermetic certifier assert container-backed execution occurred. Produced by the
container-backed lane; consumed by `certify`. Git-ignored; moved between CI jobs as a build artifact.

**Path**: `target/conformance/execution-manifest.json` (override with `certify --manifest`).
**Writer**: the container lane only — `conformance_docker_pinned` via `parity_harness::manifest_emit`.
**Reader**: `certify` only.

Schema in data-model §3.

## Producer obligations

1. **Write atomically** — temp file + `fs::rename`. A truncated manifest read concurrently by a parallel job
   would parse as `V35-incomplete` and block a release for a reason that is not real.
2. **Record every required case**, including ones that failed and ones excluded by disposition. A manifest
   that lists only successes is `V35-incomplete`, not a clean run — omission must never read as absence of a
   problem.
3. **Record the revision under test**, not the branch or tag. FR-033c exists so a manifest from another
   revision cannot be presented as evidence for this one.
4. **Record hashes at execution time**, computed from the case definitions the run actually used — not
   re-derived later, which would mask a mid-run edit.
5. **Emit on failure too.** A lane that fails still writes what it observed; the manifest is diagnostic, and
   suppressing it on red runs would hide the evidence exactly when it is most needed.

## Consumer obligations

`certify` verifies, in this order, stopping at none of them (FR-043 requires all findings in one run):

| Check | Violation |
|---|---|
| file exists and parses | `V35-absent` |
| `revision` == revision under certification | `V35-revision` |
| every required case id present in `cases` | `V35-incomplete` |
| each `caseHash`/`fixtureHash` == currently computed | `V35-stale` |
| each `outcome` in the enumeration; `excluded` resolves its `excludedBy` | `V35-unaccounted` |
| `environment` matches the profile under certification | `V35-revision` (environment mismatch detail) |

A case with `outcome: "fail"` is **not** a manifest-integrity violation. It blocks certification as an
ordinary failing case, reported against the case. Keeping these distinct matters: "the evidence is malformed"
and "the evidence says deacon diverged" need different fixes, and a maintainer reading a blocked release must
be able to tell which they have.

## What the manifest is not

- **Not committed evidence.** It is a per-run receipt, regenerated on every run. Snapshots are reviewed
  artifacts under `conformance/snapshots/`; conflating the two would mean every CI run wants to write the
  reviewed tree — the pressure FR-055 removes.
- **Not a substitute for snapshot freshness.** Both obligations hold independently (FR-033e). A fresh
  manifest cannot excuse a stale snapshot, and a fresh snapshot cannot excuse an absent manifest.
- **Not signed or attested.** The integrity properties come from the recorded revision and hashes, which are
  checkable against the repository state. A cryptographic attestation would guard against an adversary with
  write access to the artifact store — outside this feature's threat model, and it would add a key-management
  surface for no gain against the failure actually being prevented (a stale or mismatched manifest passing
  unnoticed).
