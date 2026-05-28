# Features: Option-Name Sanitization to Env Vars

A feature receives its options through the install script's environment.
The spec mandates a deterministic mapping from option-name (JSON key) to
env-var name:

1. Uppercase the option name.
2. Replace any non-`[A-Z0-9_]` character with `_`.

So:
- `my-string-option` → `MY_STRING_OPTION`
- `another.weird-key` → `ANOTHER_WEIRD_KEY`
- `flagOption` → `FLAGOPTION` (already valid; just upcased)

The sanitization is also what lets feature authors keep human-readable
JSON keys in `devcontainer.json` while writing `${MY_STRING_OPTION}` in
`install.sh`. This example wires a feature with three options covering
the three patterns and dumps the resulting env to a file we can read.

## Files

- `devcontainer.json` — provides values for each option.
- `report/devcontainer-feature.json` — declares the option schema.
- `report/install.sh` — records every uppercase env var and probes the
  expected sanitized names.

## Scenarios exercised by `exec.sh`

1. **Sanitized names present.** `MY_STRING_OPTION`,
   `ANOTHER_WEIRD_KEY`, and `FLAGOPTION` are all set inside the
   container during install.
2. **Values flow through verbatim.** `MY_STRING_OPTION=Hello, World!`,
   `ANOTHER_WEIRD_KEY=x/y/z`, `FLAGOPTION=true`.
3. **Unsanitized names are absent.** No env var literally named
   `my-string-option` exists — the install.sh probe `MYSTRINGOPTION`
   (collapsed without underscores) should be `<unset>`.

## Manual usage

```sh
deacon up --workspace-folder . --remove-existing-container
docker exec <cid> cat /usr/local/share/option-sanitization/probes
docker exec <cid> cat /usr/local/share/option-sanitization/all-env
```

## Known deacon issues this example surfaces

- [#69](https://github.com/get2knowio/deacon/issues/69) — when this
  example is run via `--config <path>` (e.g. for verification outside
  the standard `.devcontainer/devcontainer.json` layout), local feature
  paths of the form `./feature-X` are misinterpreted as OCI registry
  refs (`registry: "."`).

## Spec references

- Feature options and env-var derivation:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-features.md>
