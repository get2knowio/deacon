# Features: Local (`./path`) Feature Install

Features can be referenced three ways in `devcontainer.json`: an OCI registry
ref (`ghcr.io/owner/repo/feat:1`), a direct HTTPS tarball, or a **local
relative path** (`./my-feature`). This example exercises the local-path form —
the only one that needs no network and the one whose dispatch (`./`, `../`,
absolute) is easy to regress.

## Files

- `devcontainer.json` — references `./hello-feature` with an option override.
- `hello-feature/devcontainer-feature.json` — feature metadata with one
  `greeting` option (default `hello`).
- `hello-feature/install.sh` — writes `${GREETING} from local feature v1.0.0`
  to `/usr/local/share/local-feature/marker`.

Local feature paths resolve relative to the **config directory**. Because the
config is kept at the example root (`devcontainer.json`, not under
`.devcontainer/`), `exec.sh` points deacon at it with `--config`.

## Scenarios exercised by `exec.sh`

1. **Local feature runs.** After `up`, the marker file exists in the image.
2. **Option override applied.** The marker reads `bonjour …`, proving the
   `greeting=bonjour` option from `devcontainer.json` reached the install
   script (default would be `hello`).

## Spec references

- Feature reference formats: <https://containers.dev/implementors/features/>
