# Features: Local (`./path`) Feature Install

Features can be referenced three ways in `devcontainer.json`: an OCI registry
ref (`ghcr.io/owner/repo/feat:1`), a direct HTTPS tarball, or a **local
relative path** (`./my-feature`). This example exercises the local-path form —
the only one that needs no network and the one whose dispatch (`./`, `../`) is
easy to regress.

A local Feature must be spelled relatively and must live under `.devcontainer/`:
`devcontainer-features-distribution.md` §Locally Referenced Features says "A
local Feature may **not** be referenced by absolute path", and deacon rejects one
(#495, matching the reference CLI). Absolute paths were accepted until then
(#126); rewrite any as a path relative to the folder holding `devcontainer.json`
that lands inside `<workspace folder>/.devcontainer/` (#488).

## Files

- `devcontainer.json` — references `./.devcontainer/hello-feature` with an
  option override.
- `.devcontainer/hello-feature/devcontainer-feature.json` — feature metadata
  with one `greeting` option (default `hello`).
- `.devcontainer/hello-feature/install.sh` — writes `${GREETING} from local
  feature v1.0.0` to `/usr/local/share/local-feature/marker`.

Local feature paths resolve relative to the **config directory**, while the
containment rule anchors on `<workspace folder>/.devcontainer`. Because the
config is kept at the example root (`devcontainer.json`, not under
`.devcontainer/`), the id is spelled `./.devcontainer/hello-feature` — the two
rules together — and `exec.sh` points deacon at the config with `--config`.

## Scenarios exercised by `exec.sh`

1. **Local feature runs.** After `up`, the marker file exists in the image.
2. **Option override applied.** The marker reads `bonjour …`, proving the
   `greeting=bonjour` option from `devcontainer.json` reached the install
   script (default would be `hello`).

## Spec references

- Feature reference formats: <https://containers.dev/implementors/features/>
