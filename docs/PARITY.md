# Does deacon behave like the DevContainers CLI?

<!-- GENERATED FILE — do not edit.
     Regenerate with: cargo run -q -p deacon-conformance -- parity-page --write -->

Compared against **`@devcontainers/cli` 0.87.0** and the [containers.dev spec]\
(https://github.com/devcontainers/spec) at commit `113500f4`.

**Of 57 behaviors that both tools implement, 29 are verified to match.**
10 are verified to differ deliberately, and 4 to differ in ways we intend
to remove. A further 16 are deacon-only, with nothing to compare against.

**14 more have never been compared against the CLI at all** — 7 assumed
to match, 5 assumed to differ on purpose, 2 assumed to be behind.
These are claims, not measurements: nothing has run the reference for them, so each could turn
out to be any of the three. They are listed separately rather than folded into the totals above,
because a claim nobody has checked is not evidence of parity — and a page that counted it as such
would improve every time someone asserted something new.

## How to read this

| | Meaning |
|---|---|
| ✅ | Same as the CLI, and checked against it in this scenario |
| ◐ | Believed the same, but only deacon-side evidence here — **never compared** |
| ⚠️ | Differs on purpose |
| ❌ | Differs, and we intend to fix it |
| 🔵 | deacon-only; the CLI has no equivalent |
| · | **Not checked yet** — a real gap |
| *(blank)* | Does not arise in this scenario |

A row's Notes name the **waiver** backing a deliberate difference. A waiver's value is
that it self-invalidates: it is re-checked against the reference and fails as *stale* the
moment the difference stops reproducing, so a characterization cannot outlive the thing it
characterized.

That only holds while something re-checks it. A waiver becomes live by a test case
tolerating it; one no case names is enforced by nothing, and is marked **(unchecked)**
here. **no waiver** means a deliberate difference has none at all. Both mean the same
thing for a reader: that difference is asserted, and nothing would notice if the CLI
changed to match us tomorrow.

Columns are the scenarios a behavior was checked in: the configuration's shape
(**img**age / **dkr** Dockerfile / **cmp** Compose) crossed with how many Features it
declares (**–** none / **1** one / **many** several / **deps** several with dependency
ordering / **lock** with a lockfile).

**A column only appears where that scenario is possible.** `outdated` never resolves a
dependency graph, so it has no **deps** column at all rather than a column of `·` implying
an untested hole. So a `·` means genuinely not yet checked.

A cell says a case exercised that scenario — not that the case's assertions were strong.
The leading glyph rolls the row up; where a row is mostly `·`, that is the honest signal.

A **blank** cell means the behavior does not arise in that scenario at all — deriving a
Compose project name has no meaning under an image configuration. Blank rather than a
glyph, because there is no question there to answer.

So every `·` is a real gap: a scenario this behavior COULD be checked in and has not been.

### build

<table>
<thead>
<tr><th rowspan="2"></th><th rowspan="2">Behavior</th><th colspan="5">img</th><th colspan="5">dkr</th><th colspan="5">cmp</th><th rowspan="2">Notes</th></tr>
<tr><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th></tr>
</thead>
<tbody>
<tr><td>✅</td><td>A Dockerfile instruction that exits non-zero fails the build, and the failure is…</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>`build` reports and tags the FEATURE-EXTENDED image, so a user-supplied `--image-name`…</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>build produces a container image and reports the build outcome, matching the reference</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>◐</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
</tbody>
</table>

### doctor

<table>
<thead>
<tr><th></th><th>Behavior</th><th>img</th><th>dkr</th><th>cmp</th><th>Notes</th></tr>
</thead>
<tbody>
<tr><td>🔵</td><td>`doctor` reports host, platform, and runtime diagnostics in both a human rendering and…</td><td>🔵</td><td>🔵</td><td>🔵</td><td></td></tr>
</tbody>
</table>

### down

<table>
<thead>
<tr><th></th><th>Behavior</th><th>img</th><th>dkr</th><th>cmp</th><th>Notes</th></tr>
</thead>
<tbody>
<tr><td>🔵</td><td>`down --remove` on a Compose configuration removes the project it created, identifying…</td><td></td><td></td><td>🔵</td><td></td></tr>
<tr><td>🔵</td><td>`down` over a workspace folder that has no devcontainer configuration reports that…</td><td>🔵</td><td>·</td><td>·</td><td></td></tr>
<tr><td>🔵</td><td>`down --remove` stops and removes the workspace's container, so a subsequent command…</td><td>🔵</td><td>🔵</td><td>·</td><td></td></tr>
</tbody>
</table>

### exec

<table>
<thead>
<tr><th rowspan="2"></th><th rowspan="2">Behavior</th><th colspan="2">img</th><th colspan="2">dkr</th><th colspan="2">cmp</th><th rowspan="2">Notes</th></tr>
<tr><th>–</th><th>1</th><th>–</th><th>1</th><th>–</th><th>1</th></tr>
</thead>
<tbody>
<tr><td>✅</td><td>exec runs a command inside the target container and streams its stdout/stderr and…</td><td>✅</td><td>·</td><td>✅</td><td>·</td><td>✅</td><td>·</td><td></td></tr>
<tr><td>·</td><td>exec --container-id (no --workspace-folder/--config) recovers remoteUser and remoteEnv…</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#322</td></tr>
<tr><td>·</td><td>`exec` runs the command with the environment the user-env probe captured, with the…</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#370</td></tr>
<tr><td>⚠️</td><td>The relative order of a restored image `PATH` entry against one a Feature contributed…</td><td>·</td><td>·</td><td>·</td><td>⚠️</td><td>·</td><td>·</td><td><strong>no waiver</strong> #370</td></tr>
</tbody>
</table>

### host ca

| | Behavior | Notes |
|---|---|---|
| 🔵 | deacon can inject the host's CA certificates into the TLS trust store used for OCI… |  |

### observable state

<table>
<thead>
<tr><th rowspan="2"></th><th rowspan="2">Behavior</th><th colspan="5">img</th><th colspan="5">dkr</th><th colspan="5">cmp</th><th rowspan="2">Notes</th></tr>
<tr><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th></tr>
</thead>
<tbody>
<tr><td>⚠️</td><td>The set of Compose files a CLI composes with, and the Compose labels derived from that…</td><td></td><td></td><td></td><td></td><td></td><td></td><td></td><td></td><td></td><td></td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-compose-project-file-set</code></td></tr>
<tr><td>⚠️</td><td>deacon derives a valid, deacon-namespaced compose project name…</td><td></td><td></td><td></td><td></td><td></td><td></td><td></td><td></td><td></td><td></td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td><strong>no waiver</strong> #265</td></tr>
<tr><td>🔵</td><td>deacon stamps five identity/bookkeeping labels onto a created container that the…</td><td>🔵</td><td>·</td><td>·</td><td>·</td><td>·</td><td>🔵</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>⚠️</td><td>Both CLIs override the container command with a shell keep-alive that holds the…</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td><strong>no waiver</strong></td></tr>
<tr><td>✅</td><td>The `devcontainer.metadata` label records the image metadata the configuration and…</td><td>✅</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#373</td></tr>
<tr><td>⚠️</td><td>The BYTE FORM of the `devcontainer.metadata` label value — JSON whitespace and object…</td><td>·</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-container-metadata-label-serialization</code> #373 #394</td></tr>
<tr><td>◐</td><td>The observable container state after up (running status, labels, mounts, environment)…</td><td>◐</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>The normalized observable-state diff between deacon and the reference is empty for the…</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
</tbody>
</table>

### outdated

<table>
<thead>
<tr><th rowspan="2"></th><th rowspan="2">Behavior</th><th colspan="4">img</th><th colspan="4">dkr</th><th colspan="4">cmp</th><th rowspan="2">Notes</th></tr>
<tr><th>–</th><th>1</th><th>many</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>lock</th></tr>
</thead>
<tbody>
<tr><td>❌</td><td>`outdated` resolves the full extends chain before reporting versions, so a Feature…</td><td>·</td><td>❌</td><td>·</td><td>·</td><td>·</td><td>·</td><td>❌</td><td>·</td><td>❌</td><td>·</td><td>·</td><td>❌</td><td><code>wvr-outdated-extends-chain-features</code> <strong>(unchecked)</strong> #389</td></tr>
<tr><td>⚠️</td><td>`outdated` fails with a non-zero exit and a diagnostic naming the file when a…</td><td>·</td><td>·</td><td>·</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-outdated-malformed-lockfile-rejected</code> <strong>(unchecked)</strong> #406 #407</td></tr>
<tr><td>◐</td><td>`outdated` keys each report entry by the Feature reference the configuration DECLARED,…</td><td>·</td><td>·</td><td>◐</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#407 #411</td></tr>
<tr><td>✅</td><td>`outdated` reports each configured Feature's current, wanted, and latest version, and…</td><td>✅</td><td>◐</td><td>◐</td><td>◐</td><td>◐</td><td>✅</td><td>·</td><td>◐</td><td>✅</td><td>◐</td><td>◐</td><td>◐</td><td>#407</td></tr>
</tbody>
</table>

### ports

| | Behavior | Notes |
|---|---|---|
| 🔵 | deacon can auto-forward container ports to the host via a host-side daemon backed by a… |  |

### profiles

| | Behavior | Notes |
|---|---|---|
| 🔵 | deacon applies a user-defined profile from settings.json (selected via the global… |  |

### read configuration

<table>
<thead>
<tr><th rowspan="2"></th><th rowspan="2">Behavior</th><th colspan="5">img</th><th colspan="5">dkr</th><th colspan="5">cmp</th><th rowspan="2">Notes</th></tr>
<tr><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th></tr>
</thead>
<tbody>
<tr><td>🔵</td><td>When a link of an `extends` chain declares a Feature its base already declared at a…</td><td>·</td><td>·</td><td>🔵</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#411</td></tr>
<tr><td>⚠️</td><td>A single configuration document whose `features` map contains two keys resolving to…</td><td>·</td><td>·</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-features-duplicate-in-one-document</code> <strong>(unchecked)</strong> #411</td></tr>
<tr><td>⚠️</td><td>deacon's resolved-configuration output omits a property the author wrote as `null`,…</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-readconfig-authored-empty-omitted</code> #398</td></tr>
<tr><td>·</td><td>An explicit --config pointing at a file that does not exist is rejected (no silent…</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>Reading a well-formed devcontainer.json exits 0 and emits the resolved configuration…</td><td>✅</td><td>◐</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>◐</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>Both the `configuration` and `mergedConfiguration` documents carry `configFilePath`,…</td><td>✅</td><td>✅</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#376</td></tr>
<tr><td>❌</td><td>Configuration discovery searches exactly the three locations the spec names —…</td><td>❌</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-discovery-named-subfolder</code></td></tr>
<tr><td>✅</td><td>A JSON object with a duplicate top-level key is accepted with last-wins semantics,…</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>🔵</td><td>A cyclic extends chain (a -&gt; b -&gt; a) is detected and rejected during…</td><td>🔵</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#297</td></tr>
<tr><td>🔵</td><td>read-configuration --include-merged-configuration resolves the full extends chain…</td><td>🔵</td><td>🔵</td><td>·</td><td>·</td><td>·</td><td>🔵</td><td>🔵</td><td>·</td><td>🔵</td><td>·</td><td>·</td><td>·</td><td>🔵</td><td>·</td><td>·</td><td>#297</td></tr>
<tr><td>🔵</td><td>An extends chain pointing at a nonexistent target file is rejected during…</td><td>🔵</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#297</td></tr>
<tr><td>❌</td><td>`--include-features-configuration` reports the resolved Feature set in install order,…</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>❌</td><td>❌</td><td>·</td><td>·</td><td>·</td><td>·</td><td>❌</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>`featuresConfiguration` is reported only when Feature resolution produced at least one…</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#387</td></tr>
<tr><td>⚠️</td><td>A devcontainer.json with a hard JSONC syntax error is rejected at parse rather than…</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-malformed-json</code></td></tr>
<tr><td>·</td><td>deacon's `mergedConfiguration` omits the computed-empty properties the reference…</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-readconfig-merged-computed-empties</code> <strong>(unchecked)</strong> #398</td></tr>
<tr><td>❌</td><td>read-configuration --include-merged-configuration over the tier1 corpus emits a merged…</td><td>❌</td><td>❌</td><td>❌</td><td>❌</td><td>·</td><td>❌</td><td>❌</td><td>·</td><td>·</td><td>·</td><td>❌</td><td>❌</td><td>·</td><td>·</td><td>·</td><td>#383 #387</td></tr>
<tr><td>✅</td><td>`mergedConfiguration` reports all five plural lifecycle hook arrays…</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>·</td><td>A workspace folder with no devcontainer configuration at all is rejected rather than…</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>A `portsAttributes` / `otherPortsAttributes` entry reports the keys the author wrote…</td><td>✅</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>◐</td><td>Variable substitution reaches the object-shaped fields that carry user templates —…</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>◐</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#312</td></tr>
<tr><td>❌</td><td>Reading real-world devcontainer.json configurations from the tier1 corpus produces…</td><td>❌</td><td>❌</td><td>❌</td><td>❌</td><td>·</td><td>❌</td><td>❌</td><td>·</td><td>·</td><td>·</td><td>❌</td><td>❌</td><td>·</td><td>·</td><td>·</td><td>#383 #387</td></tr>
<tr><td>✅</td><td>Unknown / forward-compatible top-level fields are accepted and preserved verbatim in…</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>⚠️</td><td>A modelled field whose value is outside the schema's closed enum (for example…</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-unsupported-enum-values</code></td></tr>
<tr><td>✅</td><td>The reported `workspace.workspaceFolder` is the automatic source-code mount location…</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#273 #383</td></tr>
<tr><td>✅</td><td>The `workspace` section carries exactly `workspaceFolder` and the optional…</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>#376</td></tr>
<tr><td>⚠️</td><td>A `features` value that is a bare string instead of an object is rejected…</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-wrong-type-features</code></td></tr>
<tr><td>⚠️</td><td>A `forwardPorts` value that is a bare string instead of an array is rejected (typed…</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-wrong-type-forwardports</code></td></tr>
</tbody>
</table>

### run user commands

<table>
<thead>
<tr><th rowspan="2"></th><th rowspan="2">Behavior</th><th colspan="5">img</th><th colspan="5">dkr</th><th colspan="5">cmp</th><th rowspan="2">Notes</th></tr>
<tr><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th></tr>
</thead>
<tbody>
<tr><td>❌</td><td>A lifecycle hook that exits non-zero fails the run rather than being reported as a…</td><td>❌</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-ruc-completed-hooks-not-rerun</code></td></tr>
<tr><td>✅</td><td>Lifecycle hooks run in spec order — onCreate, updateContent, postCreate, postStart —…</td><td>✅</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
</tbody>
</table>

### secrets

| | Behavior | Notes |
|---|---|---|
| 🔵 | The --secrets-file flag auto-detects and accepts both a flat JSON object and a… |  |

### templates apply

<table>
<thead>
<tr><th></th><th>Behavior</th><th>img</th><th>dkr</th><th>cmp</th><th>Notes</th></tr>
</thead>
<tbody>
<tr><td>🔵</td><td>`templates apply` scaffolds the template's files into the target directory with…</td><td>🔵</td><td>🔵</td><td>🔵</td><td></td></tr>
</tbody>
</table>

### trust

| | Behavior | Notes |
|---|---|---|
| 🔵 | Host-side lifecycle hooks (initializeCommand and other workspace-resident host hooks)… |  |

### up

<table>
<thead>
<tr><th rowspan="2"></th><th rowspan="2">Behavior</th><th colspan="5">img</th><th colspan="5">dkr</th><th colspan="5">cmp</th><th rowspan="2">Notes</th></tr>
<tr><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th><th>–</th><th>1</th><th>many</th><th>deps</th><th>lock</th></tr>
</thead>
<tbody>
<tr><td>⚠️</td><td>Re-entering a workspace whose configuration has CHANGED since its container was…</td><td>·</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td><code>wvr-up-changed-config-recreates</code> <strong>(unchecked)</strong></td></tr>
<tr><td>◐</td><td>A `containerEnv` variable declared by BOTH the configuration and a Feature takes the…</td><td>◐</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>◐</td><td>·</td><td>·</td><td>·</td><td>#374</td></tr>
<tr><td>◐</td><td>`up` applies the declared container user so the container process runs as that user…</td><td>·</td><td>◐</td><td>·</td><td>·</td><td>·</td><td>◐</td><td>◐</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>up creates a container from the resolved configuration such that a subsequent exec…</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>Entrypoints contributed by multiple Features are chained and all run, in the resolved…</td><td>·</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>A Feature that cannot be installed — its install script exiting non-zero, or a…</td><td>·</td><td>✅</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>`up` installs the configuration's Features into the container it creates, in the…</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>✅</td><td>◐</td><td>·</td><td>·</td><td>·</td><td>·</td><td>◐</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>◐</td><td>A lifecycle hook runs with its working directory set to the container workspace…</td><td>◐</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>A lifecycle hook declared in the array (argv) form and one declared in the object…</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>Each declared mount is applied with both its declared source and its declared shape —…</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>◐</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>✅</td><td>A PATH segment contributed by a Feature's `containerEnv` is prepended to the image…</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
<tr><td>◐</td><td>Re-entering a workspace whose container already exists, with the SAME configuration,…</td><td>◐</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>◐</td><td>·</td><td>·</td><td>·</td><td></td></tr>
</tbody>
</table>

### upgrade

<table>
<thead>
<tr><th rowspan="2"></th><th rowspan="2">Behavior</th><th colspan="3">img</th><th colspan="3">dkr</th><th colspan="3">cmp</th><th rowspan="2">Notes</th></tr>
<tr><th>–</th><th>1</th><th>many</th><th>–</th><th>1</th><th>many</th><th>–</th><th>1</th><th>many</th></tr>
</thead>
<tbody>
<tr><td>🔵</td><td>`upgrade` regenerates the lockfile from the configuration AFTER the command-line…</td><td>·</td><td>🔵</td><td>·</td><td>🔵</td><td>·</td><td>·</td><td>·</td><td>·</td><td>🔵</td><td>#409</td></tr>
<tr><td>⚠️</td><td>`upgrade` on a configuration that declares no Features regenerates an EMPTY lockfile…</td><td>⚠️</td><td>·</td><td>·</td><td>·</td><td>·</td><td>·</td><td>⚠️</td><td>·</td><td>·</td><td><code>wvr-upgrade-empty-feature-set</code></td></tr>
<tr><td>✅</td><td>`upgrade` regenerates the devcontainer lockfile from the effective configuration's…</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>✅</td><td>·</td><td>·</td><td>·</td><td>·</td><td></td></tr>
</tbody>
</table>

---

The full conformance record — the three-axis disposition behind each row, the waivers, and the scenario-coverage accounting — lives in `conformance/registry/` and `conformance/RULES.md`. This page is generated from it.
