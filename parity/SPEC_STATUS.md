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

Of **83 recorded behaviors**:

- **3** — open nonconformance — [#430](https://github.com/get2knowio/deacon/issues/430), [#436](https://github.com/get2knowio/deacon/issues/436), [#438](https://github.com/get2knowio/deacon/issues/438)
- **10** — deacon follows the spec where the CLI does not
- **11** — documented choice
- **19** — deacon extension
- **40** — conformant and matching

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
- **Rendered compose-project state** — *partially recovered.* The primary service's
  resolved volumes, environment, mount graph and project name are now compared by
  `case-state-compose-feature-mounts` and `case-state-compose-sidecar-volume`. What stays
  dropped is the rendered project *document* — the merged Compose file each CLI hands to
  `docker compose` — which the two deliver differently (deacon on stdin, the reference via
  a temp file) and which no channel observes.
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
| A BuildKit-only build flag (`--cache-to`) is accepted and passed through, and the build still produces the configured image. | deacon extension | 1 scenario | deacon-only surface. Scoped deliberately to acceptance and pass-through: on Docker 29.6.2 with the default `docker` driver a local cache export writes nothing… |
| A Dockerfile instruction that exits non-zero fails the build, and the failure is reported at the build stage rather than as a later container-creation failure. | matches the reference | 3 scenarios | The exit code alone cannot distinguish a build failure from a container-creation failure; the declared failure phase is what makes the two different facts rather than… |
| `build` reports and tags the FEATURE-EXTENDED image, so a user-supplied `--image-name` resolves to the image with the Features installed rather than to the pre-Feature… | matches the reference | 2 scenarios | This project shipped the inverse once: the post-build Feature pass layered correctly and the user's tag still pointed at the base, which every outcome-only assertion… |
| `build` stamps `org.deacon.configHash` on the image it produces; the reference CLI sets no such label. | deacon extension | 2 scenarios | Measured against oracle 0.87.0 on fx-build-image-labels and fx-build-image-args, not inferred. A deacon-only bookkeeping label in deacon's own `org.deacon.*` namespace —… |
| The `devcontainer.metadata` label on a BUILT image records one entry per installed Feature plus the configuration pick, so a later consumer of the image can read what it… | **open nonconformance** | 2 scenarios | OPEN — filed as #436. Measured against oracle 0.87.0 on fx-build-dockerfile-feature: the reference writes `[ {"id":"./features/marker"} ]` and deacon writes… |
| build produces a container image and reports the build outcome, matching the reference. | matches the reference | 3 scenarios | Backed by case-build-image-labels-differential and case-build-image-args-differential, which compare the produced IMAGE's whole configuration between the two CLIs. Until… |
| The image a `build` produces carries what the configuration and Dockerfile authored into it — labels, environment, command and working directory — and `build.args` reach… | matches the reference | 3 scenarios | Measured against oracle 0.87.0 on fx-build-image-labels and fx-build-image-args: `Env`, `Cmd`, `Entrypoint`, `WorkingDir` and every authored label agree exactly on both… |
| `build --output type=docker,dest=<path>` exports the built image to that path and reports the export destination. | deacon extension | 1 scenario | deacon-only surface: the reference CLI has no `--output` flag on `build`, so there is nothing to compare against and the case is a spec-expectation. The claim pinned is… |
| `build --platform <os/arch>` builds for the named platform and produces a real image carrying the configuration's authored labels. | deacon extension | 1 scenario | deacon-only surface (`--platform` is not a reference `build` flag). Previously covered by an assertion that tolerated either outcome and therefore asserted nothing on… |
| `build --push` to a registry that cannot be reached fails with a non-zero exit and a diagnostic naming the push as the failing step, rather than reporting success. | matches the reference | 1 scenario | Both CLIs fail non-zero and name the push. deacon's message currently names the WRONG registry — it pushes its internal `deacon-build:<hash>` tag, which resolves to… |
| `build --push` pushes the image names the user supplied and nothing else. | **open nonconformance** | 1 scenario | OPEN — filed as #438. Measured against oracle 0.87.0 on fx-build-flags with `--push --image-name registry.invalid/conformance-push:latest`: deacon also tags the image… |

### `doctor`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| `doctor` reports host, platform, and runtime diagnostics in both a human rendering and `--json`, and does so regardless of whether the workspace carries a valid… | deacon extension | 9 scenarios | The pinned reference exposes no `doctor` command — see ext-doctor-diagnostics. Independence from the workspace configuration is the property worth pinning: `doctor` is… |

### `down`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| `down --remove` on a Compose configuration removes the project it created, identifying it by the same project name and workspace labels `up` derived. | deacon extension | 1 scenario | Same extension as bhv-down-removes-container. The risk this record exists for is a derivation that does not reproduce what `up` used — teardown then reports success and… |
| `down` over a workspace folder that has no devcontainer configuration reports that there was nothing to tear down and exits 0; it still resolves any container recorded… | deacon extension | 1 scenario | Recorded by ext-teardown-command: the spec defines no teardown command and the pinned reference exposes none, so there is no reference behavior to align with. Teardown… |
| `down --remove` stops and removes the workspace's container, so a subsequent command targeting that container fails. | deacon extension | 4 scenarios | The containers.dev spec defines no teardown command and the pinned reference exposes none, so `down` is a deacon surface with no reference analogue — see… |

### `exec`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| exec runs a command inside the target container and streams its stdout/stderr and propagates its exit code, matching the reference. | matches the reference | 8 scenarios | Backed by the declarative exec cases (case-exec-decl-tty, case-exec-decl-user, case-exec-decl-working-directory, case-exec-remote-env-propagation), which replaced the… |
| exec --container-id (no --workspace-folder/--config) recovers remoteUser and remoteEnv from the container's devcontainer.metadata label — which up stamps at create time… | matches the reference | 1 scenario | #322: up writes the merged devcontainer.metadata label (single-container, Dockerfile, and compose paths); exec/read-configuration/set-up read it back via the shared… |
| `exec` runs the command with the environment the user-env probe captured, with the PATH entries the image contributed via `ENV PATH` restored when the probe's login… | matches the reference | 1 scenario | Fixed under #370. Measured at oracle 0.87.0 against fx-exec-dockerfile-overlay: the image's `Config.Env` PATH is `/opt/conformance/bin:/usr/local/sbin:…`, and deacon's… |
| The relative order of a restored image `PATH` entry against one a Feature contributed through `/etc/profile.d`, when the probe's login shell dropped the image's. | documented choice | 1 scenario | The residual left by the #370 fix, and a deliberate consequence of how deacon fixes it. Measured at oracle 0.87.0 over fx-exec-feature-path-ordering (the image… |

### `host-ca`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| deacon can inject the host's CA certificates into the TLS trust store used for OCI registry and network access; the reference CLI does not model host-CA injection. | deacon extension | 1 scenario | Deacon-only capability (feature 016-host-ca-injection), enabled via {user_data_folder}/settings.json. Recorded by ext-host-ca-injection; backed by case-host-ca. |

### `observable-state`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| On the Compose path, `devcontainer.metadata` records mount sources as the author wrote them, with `${localWorkspaceFolder}` left unsubstituted. | matches the reference | 1 scenario | CLOSED — #437, a compose-path-only regression of #373. Both CLIs record the template on fx-state-compose-feature-mounts, verified at oracle 0.87.0. deacon built the label from the pre-substitution config all along; Docker Compose then interpolated `${localWorkspaceFolder}` in the generated override to the empty string, so `source=${localWorkspaceFolder}/sib` reached the container as `source=/sib`. The override now doubles every `$` in the values it emits, which is why the single-container path was never affected. |
| The set of Compose files a CLI composes with, and the Compose labels derived from that set (`com.docker.compose.project.config_files`, `com.docker.compose.config-hash`). | documented choice | 3 scenarios | Measured at oracle 0.87.0: both CLIs layer an override on top of the workspace's Compose file, and each delivers it differently — deacon passes it on stdin (recorded as… |
| deacon derives a valid, deacon-namespaced compose project name (deacon_<workspaceHash>_<configHash>) that docker compose always accepts and that does not collide with… | documented choice | 4 scenarios | Robustness differentiator (issue #265; docs/DIFFERENTIATORS.md). The reference derives <folder>_devcontainer verbatim, so a folder like `-myproj` yields an invalid… |
| When a Compose service's image is extended with Features, each CLI builds that image itself, so Compose's `com.docker.compose.image` label records a different content… | documented choice | 1 scenario | Measured against oracle 0.87.0 on fx-state-compose-feature-mounts: deacon `sha256:2badca10…`, reference `sha256:35612053…`. The label carries a DIGEST of an image each… |
| deacon stamps five identity/bookkeeping labels onto a created container that the reference CLI does not set at all (devcontainer.configHash, devcontainer.config_name,… | deacon extension | 10 scenarios | Measured against the pinned oracle 0.87.0 on fx-up-basic (024 Phase 5), not inferred. Newly RECORDABLE rather than newly true: the retired strip_intentional_labels rule… |
| Both CLIs override the container command with a shell keep-alive that holds the container open for exec/lifecycle work and exits cleanly on SIGTERM; the command STRINGS… | documented choice | 9 scenarios | Classified as intentional ONLY because the observable behavior was measured equal, not because the difference looks cosmetic. `docker stop`: deacon 245 ms… |
| The `devcontainer.metadata` label records the image metadata the configuration and each installed Feature contribute, with configuration values kept in the form the… | matches the reference | 7 scenarios | Surfaced by 024 US5, fixed under #373. Two differences were measured at oracle 0.87.0 and both are now closed: (1) the reference records a `{"id": "<feature>"}` entry… |
| The BYTE FORM of the `devcontainer.metadata` label value — JSON whitespace and object key order — as distinct from the entries it records. | documented choice | 3 scenarios | Split out of `bhv-container-metadata-label-content` when #373 closed the CONTENT difference and left the byte form as the only thing still differing. Two causes, both… The Compose case joined it when #437 closed the CONTENT difference there too: the Feature layer forces the reference to build the service image, and a value that reaches the container through a build is written spaced. |
| The observable container state after up (running status, labels, mounts, environment) matches the reference for the observed fixtures. | matches the reference | 1 scenario | Backed by the declarative chan-container-state cases — case-state-single-container, case-state-appport, case-state-mount-variety, case-state-dockerfile-nonroot and… |
| The normalized observable-state diff between deacon and the reference is empty for the observed fixtures. | matches the reference | 7 scenarios | Backed by the declarative chan-container-state cases, which compare the whole normalized container state between the two CLIs: case-state-single-container,… |

### `outdated`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| `outdated` resolves the full extends chain before reporting versions, so a Feature contributed by a parent link appears in the report. | deacon follows the spec, the CLI deviates | 4 scenarios | The pinned reference does not resolve `extends`, so it reports an EMPTY table and exits 0 for a workspace whose only Feature is declared one link up — a silent miss, not… |
| `outdated` fails with a non-zero exit and a diagnostic naming the file when a `devcontainer-lock.json` is PRESENT but unreadable or invalid; an ABSENT lockfile remains… | documented choice | 1 scenario | Fixes #406. deacon swallowed the read/validation error at `debug!` and continued as though no lockfile existed. That is not equivalent: the lockfile supplies `current`… |
| `outdated` keys each report entry by the Feature reference the configuration DECLARED, tag included, rather than by the canonical untagged id. | matches the reference | 1 scenario | Fixes #407 divergence 1. deacon previously keyed the report by the CANONICAL untagged id. The concrete harm was a collision: two Features declared at different tags are… |
| `outdated` reports each configured Feature's current, wanted, and latest version, and reports an empty result for a configuration that declares no Features. | matches the reference | 11 scenarios | Assertions pin `current` and `wanted`, which the configuration's exact version reference fixes; `latest` is deliberately unpinned because it is whatever the registry… |

### `ports`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| deacon can auto-forward container ports to the host via a host-side daemon backed by a per-container registry and PID markers; the reference CLI does not model a… | deacon extension | 1 scenario | Deacon-only capability (feature 015-auto-forward-ports). Recorded by ext-auto-forward-ports; backed by case-auto-forward. Uses host-side forwarded_ports.json and… |

### `profiles`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| deacon applies a user-defined profile from settings.json (selected via the global --profile flag or DEACON_PROFILE) to layer default flags and configuration; the… | deacon extension | 1 scenario | Deacon-only capability (feature 017-user-profiles). Recorded by ext-user-profiles; backed by case-user-profiles. Reads {user_data_folder}/settings.json. |

### `read-configuration`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| When a link of an `extends` chain declares a Feature its base already declared at a different version, the later link's version REPLACES the base's entry rather than… | deacon extension | 1 scenario | Fixes #411. A Feature's map key carries its version, so a child bumping an inherited version writes a DIFFERENT key; under the previous key-wise merge both survived and… |
| A single configuration document whose `features` map contains two keys resolving to the same canonical Feature id is rejected with a diagnostic naming both keys and the… | **open nonconformance** | 1 scenario | Recorded until 2026-08-02 as `spec: unspecified` + `intentional-divergence`, on the reasoning that one Feature at two versions has no coherent meaning, that within one… |
| deacon's resolved-configuration output omits a property the author wrote as `null`, reporting it identically to one the author left out, while the reference echoes the… | documented choice | 2 scenarios | Surfaced by the 024 US5 de-suppression as a THREE-state collapse (authored null, authored empty, omitted); #398 closed the empty half and this record was renamed from… |
| An explicit --config pointing at a file that does not exist is rejected (no silent discovery fallback). | matches the reference | 2 scenarios | both-reject agreement case: both CLIs reject an explicit --config that does not exist. Asserted directly by case-errors-decl-bad-config-path. The migrated… |
| Reading a well-formed devcontainer.json exits 0 and emits the resolved configuration as a single JSON document. | matches the reference | 10 scenarios |  |
| Both the `configuration` and `mergedConfiguration` documents carry `configFilePath`, an absolute VS Code URI object (`$mid`, `fsPath`, `path`, `scheme`) naming the… | matches the reference | 4 scenarios | The field is a consumer-facing contract, not spec prose: a tool reading `read-configuration` output already knows how to unmarshal a VS Code URI and knows nothing about… |
| Configuration discovery searches exactly the three locations the spec names — `.devcontainer/devcontainer.json`, `.devcontainer.json`, and… | deacon follows the spec, the CLI deviates | 4 scenarios | The spec's devcontainer-reference lists all three locations in precedence order. deacon searches all three; the pinned reference v0.87.0 does NOT search the… |
| A JSON object with a duplicate top-level key is accepted with last-wins semantics, matching the reference after pruning. | matches the reference | 1 scenario | both-accept agreement case: both CLIs accept duplicate top-level JSON keys with last-wins semantics. Asserted directly by its declarative case. The migrated… |
| A cyclic extends chain (a -> b -> a) is detected and rejected during read-configuration; the reference CLI v0.87.0 does not resolve extends and accepts. | deacon extension | 1 scenario | extends is the in-flight proposal devcontainers/spec#22 (unspecified in the pinned spec). deacon resolves eagerly and detects the loop; the reference echoes extends… |
| read-configuration --include-merged-configuration resolves the full extends chain (base image, merged containerEnv and forwardPorts); the reference CLI v0.87.0 does not… | deacon extension | 7 scenarios | Ahead-of-spec deacon capability (devcontainers/spec#22, issue #297). Characterized by ext-extends-resolution and backed by case-tier1-extends-child. See… |
| An extends chain pointing at a nonexistent target file is rejected during read-configuration; the reference CLI v0.87.0 does not resolve extends and accepts. | deacon extension | 1 scenario | extends is the in-flight proposal devcontainers/spec#22 (unspecified in the pinned spec). deacon resolves eagerly and errors on the missing target; the reference echoes… |
| `--include-features-configuration` reports the resolved Feature set in install order, and a Feature supplied by `--additional-features` joins that resolution rather than… | deacon follows the spec, the CLI deviates | 4 scenarios | Both CLIs place a dependency before its dependant, but the reported featuresConfiguration documents differ in shape: deacon reports the source identifiers, the reference… |
| `featuresConfiguration` is reported only when Feature resolution produced at least one Feature set; a configuration declaring no Features carries no such key, under… | matches the reference | 2 scenarios | deacon reported `{"featureSets": []}`, which reads as "resolution ran and produced none" where the reference says nothing at all. The value is still computed internally… |
| A devcontainer.json with a hard JSONC syntax error is rejected at parse rather than silently recovered; the reference CLI's recovering parser drops the broken property… | documented choice | 7 scenarios | deacon fails fast (constitution IV, no silent fallbacks); the reference is lenient at read-configuration. Backed by case-errors-malformed-json and wvr-malformed-json. |
| deacon's `mergedConfiguration` omits the computed-empty properties the reference synthesizes there — `containerEnv`, `remoteEnv`, `portsAttributes` and… | documented choice | allowlist only | Made VISIBLE by #398, not caused by it. `mergedConfiguration` is SYNTHESIZED rather than echoed, so unlike `configuration` an empty value there is a computed default… |
| read-configuration --include-merged-configuration over the tier1 corpus emits a merged configuration that matches the reference after normalization, except for the… | deacon follows the spec, the CLI deviates | 24 scenarios | Backed by case-merged-corpus (parity_corpus_merged over the tier1 corpus) and by the 24 declarative merged-mode variants case-merged-decl-* (023 T039). Retiring the… |
| `mergedConfiguration` reports all five plural lifecycle hook arrays (`onCreateCommands`, `updateContentCommands`, `postCreateCommands`, `postStartCommands`,… | matches the reference | 2 scenarios | The asymmetry is the whole content of this behavior and is measured, not assumed: on a config declaring three of the five hooks the reference emitted… |
| A workspace folder with no devcontainer configuration at all is rejected rather than resolved to an invented empty config. | matches the reference | 2 scenarios | both-reject agreement case: both CLIs reject a workspace with no discoverable configuration. Asserted directly by its declarative case. The migrated wvr-missing-config… |
| A `portsAttributes` / `otherPortsAttributes` entry reports the keys the author wrote and no others; an unset attribute is absent rather than explicitly null. | matches the reference | 2 scenarios | deacon serialized the whole `PortAttributes` struct, so a one-key entry came back with six extra explicit nulls and diverged on six paths at once. Distinct from the… |
| Variable substitution reaches the object-shaped fields that carry user templates — `build.args`, `containerEnv`, `mounts`, and `customizations` — and a `${localEnv:VAR}`… | matches the reference | 1 scenario | `customizations` was the field actually missed once (#312), which is why it is named in the statement rather than left to 'object-shaped fields'. Both CLIs were observed… |
| Reading real-world devcontainer.json configurations from the tier1 corpus produces resolved configurations that match the reference after normalization, except for the… | deacon follows the spec, the CLI deviates | 24 scenarios | Backed by case-tier1-corpus (parity_corpus_tier1 over the tier1 corpus) and by the 24 declarative per-workspace variants case-tier1-decl-* (023 T038). Retiring the… |
| Unknown / forward-compatible top-level fields are accepted and preserved verbatim in read-configuration output, matching the reference's fidelity. | matches the reference | 2 scenarios | both-accept agreement case; guards against silently dropping unmodeled fields (spec extensibility model). Asserted directly by its declarative case. The migrated… |
| A modelled field whose value is outside the schema's closed enum (for example `userEnvProbe: telepathy`) is rejected during configuration resolution, with a diagnostic… | deacon follows the spec, the CLI deviates | 4 scenarios | The pinned schema declares the enum closed, so a value outside it is not a valid configuration and deacon rejects it (constitution IV). The reference's… |
| The reported `workspace.workspaceFolder` is the automatic source-code mount location for THIS workspace: when the workspace folder is a subdirectory of the mounted root,… | matches the reference | 2 scenarios | devcontainerjson-reference.md defines the `workspaceFolder` default as "the automatic source code mount location", which for a workspace inside a git repository is the… |
| The `workspace` section carries exactly `workspaceFolder` and the optional `workspaceMount`, and no other field. | matches the reference | 3 scenarios | The output envelope is a reference-CLI contract, not a spec-defined shape, so `spec` is `unspecified` and the decision is to match the reference rather than to follow… |
| A `features` value that is a bare string instead of an object is rejected (type-strict, matching the schema shape); the reference keeps the raw JSON and accepts. | deacon follows the spec, the CLI deviates | 3 scenarios | The schema declares `features` an object; a bare string is not a valid configuration, and deacon enforces the declared shape (constitution IV) as it already did for the… |
| A `forwardPorts` value that is a bare string instead of an array is rejected (typed deserialization); the reference keeps the raw JSON and accepts. | deacon follows the spec, the CLI deviates | 4 scenarios | The schema declares `forwardPorts` an array; a bare string is not a valid configuration, and deacon's typed deserialization rejects it. The reference keeps the raw JSON… |

### `run-user-commands`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| A lifecycle hook that exits non-zero fails the run rather than being reported as a successful setup. | deacon follows the spec, the CLI deviates | 7 scenarios | The failing hook arrives as a command-line override so the preceding `up` can succeed: a hook failure during creation is a different case, and conflating the two would… |
| Lifecycle hooks run in spec order — onCreate, updateContent, postCreate, postStart — and a Feature-contributed hook runs exactly once alongside the configuration's own. | matches the reference | 6 scenarios | Observed by having each hook append its own name to a file inside the container, so the file IS the order. `grep -c` rather than a presence check pins the Feature hook's… |

### `secrets`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| The --secrets-file flag auto-detects and accepts both a flat JSON object and a .env-format KEY=VALUE file; the reference CLI models only JSON and rejects the .env form. | deacon extension | 1 scenario | Intentional Deacon extension (a strict superset). Recorded by ext-secrets-file-env-format; backed by case-secrets-dotenv. See docs/DIFFERENTIATORS.md and… |

### `templates-apply`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| `templates apply` scaffolds the template's files into the target directory with `${templateOption:...}` placeholders substituted — from a supplied option, or from the… | matches the reference | 8 scenarios | The spec's devcontainer-templates document defines option substitution; the reference implements `templates apply` but its required --template-id accepts only an OCI… |

### `trust`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| Host-side lifecycle hooks (initializeCommand and other workspace-resident host hooks) are gated behind an explicit workspace-trust opt-in before any host execution. | deacon extension | 1 scenario | Deacon-specific security gate the upstream spec does not mandate (--trust-workspace[-persist], a persisted allowlist, DEACON_NO_PROMPT=1 to fail closed). Recorded by… |

### `up`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| Re-entering a workspace whose configuration has CHANGED since its container was created provisions a new container for the new configuration rather than reattaching to… | documented choice | 1 scenario | Measured directly at oracle 0.87.0 against fx-up-stale-config: two `up` runs whose documents differ only in `name` and `containerEnv` return the IDENTICAL containerId… |
| A `containerEnv` variable declared by BOTH the configuration and a Feature takes the configuration's value in the created container; variables contributed by only one… | matches the reference | 4 scenarios | Authored in 024 US5 after the de-suppression found deacon on the wrong side: `commands/up/container.rs` merged the Feature-built image's containerEnv over the… |
| `up` applies the declared container user so the container process runs as that user with that user's UID and GID, whether the user is created by the image or by a… | matches the reference | 3 scenarios | Observed twice over: the container's declared user spec (`Config.User`, parsed into name/uid/group/gid by the derived `userSpec`) and the EFFECT, read by an exec running… |
| up creates a container from the resolved configuration such that a subsequent exec observes the expected environment and remote user, matching the reference. | matches the reference | 7 scenarios | Backed by the declarative case-up-exec-decl-traditional. It previously shared a single reported outcome with bhv-exec-container-id-metadata on the hand-written… |
| Entrypoints contributed by multiple Features are chained and all run, in the resolved install order, before the container's own command. | matches the reference | 2 scenarios | The two CLIs build the chain differently — deacon mounts a generated `/devcontainer/entrypoint-wrapper.sh`, the reference inlines the scripts into the container command… |
| A Feature that cannot be installed — its install script exiting non-zero, or a dependency graph with no resolvable order — fails `up` rather than producing a container… | matches the reference | 2 scenarios | Authored by the 024 US4 error-path tier. Deliberately separate from `bhv-up-feature-install-order`: that claim is about WHICH Features run and in what order on the… |
| `up` installs the configuration's Features into the container it creates, in the resolved install order, across image, Dockerfile, and Compose configuration sources. | matches the reference | 5 scenarios | Evidenced by a live differential on the outcome plus a spec-expectation case that reads the marker each Feature's install.sh writes INSIDE the container — the lesson… |
| A lifecycle hook runs with its working directory set to the container workspace folder, so a command that resolves a relative path resolves it against the project… | matches the reference | 1 scenario | Authored by 024 T150 out of the one remaining un-restated `non-testable` clause: features-contribute-lifecycle-scripts' "As with all lifecycle hooks, commands are… |
| A lifecycle hook declared in the array (argv) form and one declared in the object (named commands) form are both executed, the object form running every named command. | matches the reference | 3 scenarios | Each form writes its own marker file into the bind-mounted workspace, so the observation is the file rather than the exit code — a hook that silently did not run leaves… |
| Each declared mount is applied with both its declared source and its declared shape — the mount type and the read-only flag — and two mounts differing only in source are… | matches the reference | 3 scenarios | FR-053's two axes are separated by construction in the fixture: `/mnt/ro` and `/mnt/rw` have the SAME shape and different sources, `/mnt/rw` and `/mnt/vol` have the same… |
| A PATH segment contributed by a Feature's `containerEnv` is prepended to the image PATH, and the resulting PATH is observable segment-by-segment in the created container. | matches the reference | 2 scenarios | PATH was captured but NOT compared until 024 US5: `drop_noise_env` removed it at capture, so the declarative container-state channel never saw it. It is compared… |
| Re-entering a workspace whose container already exists, with the SAME configuration, reattaches to it rather than creating a second one. | matches the reference | 2 scenarios | Asserted as a metamorphic relationship (`first-create-vs-restart`) rather than against a fixed output, because what is being pinned is the relation between the two runs… |

### `upgrade`

| Behavior | Status | Evidence | Notes |
|---|---|---|---|
| `upgrade` regenerates the lockfile from the configuration AFTER the command-line overlays are applied: `--override-config` replaces the base document and… | deacon extension | 3 scenarios | The reference CLI's `upgrade` accepts NEITHER flag — `--merge-config` does not exist anywhere in its surface, and its `upgrade` has no `--override-config` even though… |
| `upgrade` on a configuration that declares no Features regenerates an EMPTY lockfile and exits 0. | deacon follows the spec, the CLI deviates | 3 scenarios | devcontainer-lockfile.md defines the lockfile as "a JSON object with a `features` property" and nowhere makes an empty Feature set an error, so `{"features":{}}` is the… |
| `upgrade` regenerates the devcontainer lockfile from the effective configuration's resolved Feature set, including Features contributed by a parent link of an extends… | matches the reference | 3 scenarios | Both CLIs resolve the same pinned Feature reference to the same digest and integrity, so the comparison is on the lockfile document itself rather than on the exit code.… |
