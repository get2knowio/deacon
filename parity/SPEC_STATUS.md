# Does deacon behave like the DevContainers CLI?

Compared against **`@devcontainers/cli` 0.87.0** and the [containers.dev spec]
(https://github.com/devcontainers/spec) at commit `113500f4`.

This is the durable record of what we know about deacon's behavior relative to the
reference. It is **hand-maintained**: when a scenario in `parity/cases/` teaches us
something new, the row changes here in the same commit.

## How to read a row

| Status | Meaning |
|---|---|
| **Conformant and matching** | The spec defines it, deacon follows it, and the reference agrees. Nothing owed. |
| **deacon follows the spec where the CLI does not** | An observable difference in which deacon is the conformant side. The reference's deviation, not work we owe. |
| **Documented choice** | The spec is silent and the two tools differ deliberately. Each is allowlisted with its reasoning in `parity/ALLOWLIST.json`. |
| **deacon extension** | Capability the reference has no equivalent for. Nothing to compare against. |
| **Open nonconformance** | deacon is the wrong side and we know it. Always carries an issue link. |

A status is a claim backed by a scenario or an allowlist entry — never by argument
alone. Where a row has no scenario yet, it says so.

## Summary

Of **73 recorded behaviors**:

- **1** — open nonconformance ([#430](https://github.com/get2knowio/deacon/issues/430))
- **10** — deacon follows the spec where the CLI does not
- **10** — documented choice
- **15** — deacon extension
- **37** — conformant and matching

## Coverage this document does *not* claim

Combinatorial coverage of the scenario space is **no longer tracked**. The previous
model counted obligations across a six-dimension scenario grid and dispositioned each
one; it measured a hole rather than closing it, and cost more to maintain than the
measurement was worth. What remains is the scenario suite itself: if a behavior matters,
it gets a scenario.

Four areas of coverage were **consciously dropped** when the parity corpora were
retired, rather than ported:

- **Cross-CLI container handoff** — running the reference's `up` then deacon's `up` over
  one workspace and asserting the second provisions its own container.
- **Rendered compose-project state** — the primary service's resolved image, volumes,
  environment and project name as each CLI renders them.
- **Runtime-versus-merged cross-check** — relating one CLI's
  `read-configuration --include-merged-configuration` to the container it actually made.
- **Intra-deacon comparison** — diffing two deacon runs (single-container versus compose)
  against each other, with no reference side.

Each needed machinery no declarative channel provides, and none had produced a finding.
They are recorded here so their absence is a decision on the record rather than an
oversight nobody noticed.

## Behaviors by area

### `build`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| A Dockerfile instruction that exits non-zero fails the build, and the failure is reported at the build stage rather than as a later container-creation failure. | matches the reference | 3 scenarios | The exit code alone cannot distinguish a build failure from a container-creation failure; the declared failure phase is what makes the two different facts rather than the same number. |
| `build` reports and tags the FEATURE-EXTENDED image, so a user-supplied `--image-name` resolves to the image with the Features installed rather than to the… | matches the reference | 2 scenarios | This project shipped the inverse once: the post-build Feature pass layered correctly and the user's tag still pointed at the base, which every outcome-only assertion reported as success. The case… |
| build produces a container image and reports the build outcome, matching the reference. | matches the reference | 4 scenarios | Backed by case-build-parity (parity_build). |

### `doctor`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| `doctor` reports host, platform, and runtime diagnostics in both a human rendering and `--json`, and does so regardless of whether the workspace carries a valid… | deacon extension | 9 scenarios | The pinned reference exposes no `doctor` command — see ext-doctor-diagnostics. Independence from the workspace configuration is the property worth pinning: `doctor` is what a user reaches for when… |

### `down`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| `down --remove` on a Compose configuration removes the project it created, identifying it by the same project name and workspace labels `up` derived. | deacon extension | 1 scenario | Same extension as bhv-down-removes-container. The risk this record exists for is a derivation that does not reproduce what `up` used — teardown then reports success and leaves the project running,… |
| `down` over a workspace folder that has no devcontainer configuration reports that there was nothing to tear down and exits 0; it still resolves any container… | deacon extension | 1 scenario | Recorded by ext-teardown-command: the spec defines no teardown command and the pinned reference exposes none, so there is no reference behavior to align with. Teardown is IDEMPOTENT by design —… |
| `down --remove` stops and removes the workspace's container, so a subsequent command targeting that container fails. | deacon extension | 4 scenarios | The containers.dev spec defines no teardown command and the pinned reference exposes none, so `down` is a deacon surface with no reference analogue — see ext-teardown-command. Removal is asserted… |

### `exec`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| The relative order of a restored image `PATH` entry against one a Feature contributed through `/etc/profile.d`, when the probe's login shell dropped the image's. | documented choice | 1 scenario | The residual left by the #370 fix, and a deliberate consequence of how deacon fixes it. Measured at oracle 0.87.0 over fx-exec-feature-path-ordering (the image contributes `/opt/image/bin` via… |
| exec runs a command inside the target container and streams its stdout/stderr and propagates its exit code, matching the reference. | matches the reference | 9 scenarios | Backed by case-exec-parity (parity_exec). |
| exec --container-id (no --workspace-folder/--config) recovers remoteUser and remoteEnv from the container's devcontainer.metadata label — which up stamps at create… | matches the reference | 1 scenario | #322: up writes the merged devcontainer.metadata label (single-container, Dockerfile, and compose paths); exec/read-configuration/set-up read it back via the shared… |
| `exec` runs the command with the environment the user-env probe captured, with the PATH entries the image contributed via `ENV PATH` restored when the probe's login… | matches the reference | 1 scenario | Fixed under #370. Measured at oracle 0.87.0 against fx-exec-dockerfile-overlay: the image's `Config.Env` PATH is `/opt/conformance/bin:/usr/local/sbin:…`, and deacon's `exec` now reports exactly… |

### `host-ca`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| deacon can inject the host's CA certificates into the TLS trust store used for OCI registry and network access; the reference CLI does not model host-CA injection. | deacon extension | 1 scenario | Deacon-only capability (feature 016-host-ca-injection), enabled via {user_data_folder}/settings.json. Recorded by ext-host-ca-injection; backed by case-host-ca. |

### `observable-state`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| The set of Compose files a CLI composes with, and the Compose labels derived from that set (`com.docker.compose.project.config_files`, `com.docker.compose.config-hash`). | documented choice | 1 scenario | Both CLIs layer a generated override on the workspace's Compose file and deliver it differently: deacon writes it to Compose's stdin, which Compose records as `-` in… |
| deacon derives a valid, deacon-namespaced compose project name (deacon_<workspaceHash>_<configHash>) that docker compose always accepts and that does not collide with… | documented choice | 3 scenarios | Robustness differentiator (issue #265; docs/DIFFERENTIATORS.md). The reference derives <folder>_devcontainer verbatim, so a folder like `-myproj` yields an invalid --project-name docker compose… |
| Both CLIs override the container command with a shell keep-alive that holds the container open for exec/lifecycle work and exits cleanly on SIGTERM; the command… | documented choice | 6 scenarios | Classified as intentional ONLY because the observable behavior was measured equal, not because the difference looks cosmetic. `docker stop`: deacon 245 ms (single-container) / 138 ms (compose),… |
| The BYTE FORM of the `devcontainer.metadata` label value — JSON whitespace and object key order — as distinct from the entries it records. | documented choice | 2 scenarios | The residual left after #373 closed the CONTENT difference in `devcontainer.metadata`: both values now record the same entries, and only their BYTE FORM differs. Two causes, both measured at… |
| deacon stamps five identity/bookkeeping labels onto a created container that the reference CLI does not set at all (devcontainer.configHash, devcontainer.config_name,… | deacon extension | 7 scenarios | Measured against the pinned oracle 0.87.0 on fx-up-basic (024 Phase 5), not inferred. Newly RECORDABLE rather than newly true: the retired strip_intentional_labels rule dropped the whole… |
| The `devcontainer.metadata` label records the image metadata the configuration and each installed Feature contribute, with configuration values kept in the form the… | matches the reference | 6 scenarios | Surfaced by 024 US5, fixed under #373. Two differences were measured at oracle 0.87.0 and both are now closed: (1) the reference records a `{"id": "<feature>"}` entry for every installed Feature,… |
| The observable container state after up (running status, labels, mounts, environment) matches the reference for the observed fixtures. | matches the reference | 2 scenarios | Backed by case-observable-state (parity_observable_state). |
| The normalized observable-state diff between deacon and the reference is empty for the observed fixtures. | matches the reference | 5 scenarios | Backed by case-state-diff (parity_state_diff). |

### `outdated`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| `outdated` resolves the full extends chain before reporting versions, so a Feature contributed by a parent link appears in the report. | follows spec; CLI differs | 4 scenarios | The pinned reference does not resolve `extends`, so it reports an EMPTY table and exits 0 for a workspace whose only Feature is declared one link up — a silent miss, not an error (observed… |
| `outdated` fails with a non-zero exit and a diagnostic naming the file when a `devcontainer-lock.json` is PRESENT but unreadable or invalid; an ABSENT lockfile… | documented choice | 1 scenario | deacon reads `devcontainer-lock.json` for `outdated` and lets it supply the reported `current` version; the reference CLI does not read it at all for this subcommand, so a malformed lockfile… |
| `outdated` keys each report entry by the Feature reference the configuration DECLARED, tag included, rather than by the canonical untagged id. | matches the reference | 1 scenario | Fixes #407 divergence 1. deacon previously keyed the report by the CANONICAL untagged id. The concrete harm was a collision: two Features declared at different tags are distinct keys in… |
| `outdated` reports each configured Feature's current, wanted, and latest version, and reports an empty result for a configuration that declares no Features. | matches the reference | 11 scenarios | Assertions pin `current` and `wanted`, which the configuration's exact version reference fixes; `latest` is deliberately unpinned because it is whatever the registry publishes next and an… |

### `ports`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| deacon can auto-forward container ports to the host via a host-side daemon backed by a per-container registry and PID markers; the reference CLI does not model a… | deacon extension | 1 scenario | Deacon-only capability (feature 015-auto-forward-ports). Recorded by ext-auto-forward-ports; backed by case-auto-forward. Uses host-side forwarded_ports.json and forward_daemon_<container_id>… |

### `profiles`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| deacon applies a user-defined profile from settings.json (selected via the global --profile flag or DEACON_PROFILE) to layer default flags and configuration; the… | deacon extension | 1 scenario | Deacon-only capability (feature 017-user-profiles). Recorded by ext-user-profiles; backed by case-user-profiles. Reads {user_data_folder}/settings.json. |

### `read-configuration`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| A single configuration document whose `features` map contains two keys resolving to the same canonical Feature id is rejected with a diagnostic naming both keys and… | **open — deacon is wrong** ([#430](https://github.com/get2knowio/deacon/issues/430)) | 1 scenario | Recorded until 2026-08-02 as `spec: unspecified` + `intentional-divergence`, on the reasoning that one Feature at two versions has no coherent meaning, that within one document there is 'nothing… |
| Configuration discovery searches exactly the three locations the spec names — `.devcontainer/devcontainer.json`, `.devcontainer.json`, and… | follows spec; CLI differs | 4 scenarios | The spec's devcontainer-reference lists all three locations in precedence order. deacon searches all three; the pinned reference v0.87.0 does NOT search the one-level-deep sub-folder without an… |
| `--include-features-configuration` reports the resolved Feature set in install order, and a Feature supplied by `--additional-features` joins that resolution rather… | follows spec; CLI differs | 4 scenarios | Both CLIs place a dependency before its dependant, but the reported featuresConfiguration documents differ in shape: deacon reports the source identifiers, the reference additionally reports cache… |
| read-configuration --include-merged-configuration over the tier1 corpus emits a merged configuration that matches the reference after normalization, except for the… | follows spec; CLI differs | 24 scenarios | Backed by case-merged-corpus (parity_corpus_merged over the tier1 corpus) and by the 24 declarative merged-mode variants case-merged-decl-* (023 T039). Retiring the blanket `prune` normalizer (023… |
| Reading real-world devcontainer.json configurations from the tier1 corpus produces resolved configurations that match the reference after normalization, except for… | follows spec; CLI differs | 24 scenarios | Backed by case-tier1-corpus (parity_corpus_tier1 over the tier1 corpus) and by the 24 declarative per-workspace variants case-tier1-decl-* (023 T038). Retiring the blanket `prune` normalizer (023… |
| A modelled field whose value is outside the schema's closed enum (for example `userEnvProbe: telepathy`) is rejected during configuration resolution, with a… | follows spec; CLI differs | 4 scenarios | The pinned schema declares the enum closed, so a value outside it is not a valid configuration and deacon rejects it (constitution IV). The reference's read-configuration is a lenient… |
| A `features` value that is a bare string instead of an object is rejected (type-strict, matching the schema shape); the reference keeps the raw JSON and accepts. | follows spec; CLI differs | 3 scenarios | The schema declares `features` an object; a bare string is not a valid configuration, and deacon enforces the declared shape (constitution IV) as it already did for the typed `forwardPorts`. The… |
| A `forwardPorts` value that is a bare string instead of an array is rejected (typed deserialization); the reference keeps the raw JSON and accepts. | follows spec; CLI differs | 4 scenarios | The schema declares `forwardPorts` an array; a bare string is not a valid configuration, and deacon's typed deserialization rejects it. The reference keeps the raw JSON and accepts, so the… |
| deacon's resolved-configuration output omits a property the author wrote as `null`, reporting it identically to one the author left out, while the reference echoes… | documented choice | 2 scenarios | Surfaced by the 024 US5 de-suppression, which narrowed `drop_absent_optional` to deacon's side. While the rule ran on BOTH sides, an authored null, an authored empty collection and an omitted… |
| A devcontainer.json with a hard JSONC syntax error is rejected at parse rather than silently recovered; the reference CLI's recovering parser drops the broken… | documented choice | 7 scenarios | JSONC with a hard syntax error. deacon rejects at parse; the reference uses a recovering jsonc parser that accepts and drops whatever follows the break. ADJUDICATED 2026-08-02 and kept, with the… |
| deacon's `mergedConfiguration` omits the computed-empty properties the reference synthesizes there — `containerEnv`, `remoteEnv`, `portsAttributes` and… | documented choice | allowlist only | Made visible by #398 rather than caused by it. `mergedConfiguration` is SYNTHESIZED by both CLIs, so an empty value there is a computed default rather than authorship information — which is… |
| When a link of an `extends` chain declares a Feature its base already declared at a different version, the later link's version REPLACES the base's entry rather than… | deacon extension | 1 scenario | Fixes #411. A Feature's map key carries its version, so a child bumping an inherited version writes a DIFFERENT key; under the previous key-wise merge both survived and everything downstream… |
| A cyclic extends chain (a -> b -> a) is detected and rejected during read-configuration; the reference CLI v0.87.0 does not resolve extends and accepts. | deacon extension | 1 scenario | extends is the in-flight proposal devcontainers/spec#22 (unspecified in the pinned spec). deacon resolves eagerly and detects the loop; the reference echoes extends literally. Characterized by… |
| read-configuration --include-merged-configuration resolves the full extends chain (base image, merged containerEnv and forwardPorts); the reference CLI v0.87.0 does… | deacon extension | 7 scenarios | Ahead-of-spec deacon capability (devcontainers/spec#22, issue #297). Characterized by ext-extends-resolution and backed by case-tier1-extends-child. See docs/DIFFERENTIATORS.md. The duplicate… |
| An extends chain pointing at a nonexistent target file is rejected during read-configuration; the reference CLI v0.87.0 does not resolve extends and accepts. | deacon extension | 1 scenario | extends is the in-flight proposal devcontainers/spec#22 (unspecified in the pinned spec). deacon resolves eagerly and errors on the missing target; the reference echoes extends literally.… |
| An explicit --config pointing at a file that does not exist is rejected (no silent discovery fallback). | matches the reference | 2 scenarios | both-reject agreement case: both CLIs reject an explicit --config that does not exist. Asserted directly by case-errors-decl-bad-config-path. The migrated wvr-bad-config-path record was retired… |
| Reading a well-formed devcontainer.json exits 0 and emits the resolved configuration as a single JSON document. | matches the reference | 10 scenarios |  |
| Both the `configuration` and `mergedConfiguration` documents carry `configFilePath`, an absolute VS Code URI object (`$mid`, `fsPath`, `path`, `scheme`) naming the… | matches the reference | 4 scenarios | The field is a consumer-facing contract, not spec prose: a tool reading `read-configuration` output already knows how to unmarshal a VS Code URI and knows nothing about a deacon-specific… |
| A JSON object with a duplicate top-level key is accepted with last-wins semantics, matching the reference after pruning. | matches the reference | 1 scenario | both-accept agreement case: both CLIs accept duplicate top-level JSON keys with last-wins semantics. Asserted directly by its declarative case. The migrated wvr-duplicate-keys record was retired… |
| `featuresConfiguration` is reported only when Feature resolution produced at least one Feature set; a configuration declaring no Features carries no such key, under… | matches the reference | 2 scenarios | deacon reported `{"featureSets": []}`, which reads as "resolution ran and produced none" where the reference says nothing at all. The value is still computed internally — the merged configuration… |
| `mergedConfiguration` reports all five plural lifecycle hook arrays (`onCreateCommands`, `updateContentCommands`, `postCreateCommands`, `postStartCommands`,… | matches the reference | 2 scenarios | The asymmetry is the whole content of this behavior and is measured, not assumed: on a config declaring three of the five hooks the reference emitted `updateContentCommands: []` and… |
| A workspace folder with no devcontainer configuration at all is rejected rather than resolved to an invented empty config. | matches the reference | 2 scenarios | both-reject agreement case: both CLIs reject a workspace with no discoverable configuration. Asserted directly by its declarative case. The migrated wvr-missing-config record was retired… |
| A `portsAttributes` / `otherPortsAttributes` entry reports the keys the author wrote and no others; an unset attribute is absent rather than explicitly null. | matches the reference | 2 scenarios | deacon serialized the whole `PortAttributes` struct, so a one-key entry came back with six extra explicit nulls and diverged on six paths at once. Distinct from the top-level absent-optional… |
| Variable substitution reaches the object-shaped fields that carry user templates — `build.args`, `containerEnv`, `mounts`, and `customizations` — and a… | matches the reference | 1 scenario | `customizations` was the field actually missed once (#312), which is why it is named in the statement rather than left to 'object-shaped fields'. Both CLIs were observed producing identical… |
| Unknown / forward-compatible top-level fields are accepted and preserved verbatim in read-configuration output, matching the reference's fidelity. | matches the reference | 2 scenarios | both-accept agreement case; guards against silently dropping unmodeled fields (spec extensibility model). Asserted directly by its declarative case. The migrated wvr-unknown-field-preserved record… |
| The reported `workspace.workspaceFolder` is the automatic source-code mount location for THIS workspace: when the workspace folder is a subdirectory of the mounted… | matches the reference | 2 scenarios | devcontainerjson-reference.md defines the `workspaceFolder` default as "the automatic source code mount location", which for a workspace inside a git repository is the workspace's own path under… |
| The `workspace` section carries exactly `workspaceFolder` and the optional `workspaceMount`, and no other field. | matches the reference | 3 scenarios | The output envelope is a reference-CLI contract, not a spec-defined shape, so `spec` is `unspecified` and the decision is to match the reference rather than to follow prose. deacon additionally… |

### `run-user-commands`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| A lifecycle hook that exits non-zero fails the run rather than being reported as a successful setup. | follows spec; CLI differs | 7 scenarios | The failing hook arrives as a command-line override so the preceding `up` can succeed: a hook failure during creation is a different case, and conflating the two would leave neither pinned.… |
| Lifecycle hooks run in spec order — onCreate, updateContent, postCreate, postStart — and a Feature-contributed hook runs exactly once alongside the configuration's own. | matches the reference | 6 scenarios | Observed by having each hook append its own name to a file inside the container, so the file IS the order. `grep -c` rather than a presence check pins the Feature hook's ONCE, which presence cannot. |

### `secrets`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| The --secrets-file flag auto-detects and accepts both a flat JSON object and a .env-format KEY=VALUE file; the reference CLI models only JSON and rejects the .env form. | deacon extension | 1 scenario | Intentional Deacon extension (a strict superset). Recorded by ext-secrets-file-env-format; backed by case-secrets-dotenv. See docs/DIFFERENTIATORS.md and crates/core/src/secrets.rs. |

### `templates-apply`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| `templates apply` scaffolds the template's files into the target directory with `${templateOption:...}` placeholders substituted — from a supplied option, or from the… | matches the reference | 8 scenarios | The spec's devcontainer-templates document defines option substitution; the reference implements `templates apply` but its required --template-id accepts only an OCI registry reference while… |

### `trust`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| Host-side lifecycle hooks (initializeCommand and other workspace-resident host hooks) are gated behind an explicit workspace-trust opt-in before any host execution. | deacon extension | 1 scenario | Deacon-specific security gate the upstream spec does not mandate (--trust-workspace[-persist], a persisted allowlist, DEACON_NO_PROMPT=1 to fail closed). Recorded by ext-workspace-trust-gate;… |

### `up`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| Re-entering a workspace whose configuration has CHANGED since its container was created provisions a new container for the new configuration rather than reattaching… | documented choice | 1 scenario | Surfaced by the 024 US3 stale-config re-entry case (hrt-up-image-single-feature-stale-config). Re-entering a workspace whose configuration has CHANGED since its container was created: the… |
| A `containerEnv` variable declared by BOTH the configuration and a Feature takes the configuration's value in the created container; variables contributed by only one… | matches the reference | 4 scenarios | Authored in 024 US5 after the de-suppression found deacon on the wrong side: `commands/up/container.rs` merged the Feature-built image's containerEnv over the configuration's with `extend`, so the… |
| `up` applies the declared container user so the container process runs as that user with that user's UID and GID, whether the user is created by the image or by a… | matches the reference | 3 scenarios | Observed twice over: the container's declared user spec (`Config.User`, parsed into name/uid/group/gid by the derived `userSpec`) and the EFFECT, read by an exec running `id -u`/`id -g`/`id -un`… |
| up creates a container from the resolved configuration such that a subsequent exec observes the expected environment and remote user, matching the reference. | matches the reference | 8 scenarios | Backed by case-up-exec-parity (parity_up_exec, a single reported outcome shared with bhv-exec-container-id-metadata) and, independently, by the declarative case-up-exec-decl-traditional (023 T040). |
| Entrypoints contributed by multiple Features are chained and all run, in the resolved install order, before the container's own command. | matches the reference | 2 scenarios | The two CLIs build the chain differently — deacon mounts a generated `/devcontainer/entrypoint-wrapper.sh`, the reference inlines the scripts into the container command — so the raw… |
| A Feature that cannot be installed — its install script exiting non-zero, or a dependency graph with no resolvable order — fails `up` rather than producing a… | matches the reference | 2 scenarios | Authored by the 024 US4 error-path tier. Deliberately separate from `bhv-up-feature-install-order`: that claim is about WHICH Features run and in what order on the success path, and an… |
| `up` installs the configuration's Features into the container it creates, in the resolved install order, across image, Dockerfile, and Compose configuration sources. | matches the reference | 5 scenarios | Evidenced by a live differential on the outcome plus a spec-expectation case that reads the marker each Feature's install.sh writes INSIDE the container — the lesson from the --image-name defect,… |
| A lifecycle hook runs with its working directory set to the container workspace folder, so a command that resolves a relative path resolves it against the project… | matches the reference | 1 scenario | Authored by 024 T150 out of the one remaining un-restated `non-testable` clause: features-contribute-lifecycle-scripts' "As with all lifecycle hooks, commands are executed from the context (cwd)… |
| A lifecycle hook declared in the array (argv) form and one declared in the object (named commands) form are both executed, the object form running every named command. | matches the reference | 3 scenarios | Each form writes its own marker file into the bind-mounted workspace, so the observation is the file rather than the exit code — a hook that silently did not run leaves an exit code of 0 behind… |
| Each declared mount is applied with both its declared source and its declared shape — the mount type and the read-only flag — and two mounts differing only in source… | matches the reference | 3 scenarios | FR-053's two axes are separated by construction in the fixture: `/mnt/ro` and `/mnt/rw` have the SAME shape and different sources, `/mnt/rw` and `/mnt/vol` have the same read-only flag and… |
| A PATH segment contributed by a Feature's `containerEnv` is prepended to the image PATH, and the resulting PATH is observable segment-by-segment in the created container. | matches the reference | 2 scenarios | PATH was captured but NOT compared until 024 US5: `drop_noise_env` removed it at capture, so the declarative container-state channel never saw it. It is compared segment-wise (the derived… |
| Re-entering a workspace whose container already exists, with the SAME configuration, reattaches to it rather than creating a second one. | matches the reference | 2 scenarios | Asserted as a metamorphic relationship (`first-create-vs-restart`) rather than against a fixed output, because what is being pinned is the relation between the two runs and not either run's… |

### `upgrade`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| `upgrade` on a configuration that declares no Features regenerates an EMPTY lockfile and exits 0. | follows spec; CLI differs | 3 scenarios | devcontainer-lockfile.md defines the lockfile as "a JSON object with a `features` property" and nowhere makes an empty Feature set an error, so `{"features":{}}` is the document the spec describes… |
| `upgrade` regenerates the lockfile from the configuration AFTER the command-line overlays are applied: `--override-config` replaces the base document and… | deacon extension | 3 scenarios | The reference CLI's `upgrade` accepts NEITHER flag — `--merge-config` does not exist anywhere in its surface, and its `upgrade` has no `--override-config` even though its `up` and… |
| `upgrade` regenerates the devcontainer lockfile from the effective configuration's resolved Feature set, including Features contributed by a parent link of an extends… | matches the reference | 3 scenarios | Both CLIs resolve the same pinned Feature reference to the same digest and integrity, so the comparison is on the lockfile document itself rather than on the exit code. An implementation that… |

