# Template: `optionalPaths` Demonstration

The DevContainer Templates spec defines `optionalPaths` — an array of
file paths inside the template that the consumer can **opt in or out
of** at apply time. Required files in `files` are always copied;
`optionalPaths` are surfaced to the user (typically by the IDE
applying the template) so they can decline files that don't apply to
their project (CI workflow, contribution guide, optional helper
scripts).

## Files

- `devcontainer-template.json` — declares both `files` (full set) and
  `optionalPaths` (the three opt-in subset). `projectName` is a
  string option whose default `demo-app` propagates via
  `${templateOption:projectName}` substitution.
- `.devcontainer/devcontainer.json`, `PROJECT_README.md`,
  `scripts/setup.sh` — required files (in `files` but NOT in
  `optionalPaths`). Always copied.
- `scripts/db-migrate.sh`, `docs/CONTRIBUTING.md`,
  `.github/workflows/ci.yml` — optional files (listed in both).

`PROJECT_README.md` (not this file) is the substitution target that
gets copied to the applied destination; `README.md` here documents the
example and isn't part of the template's `files[]`.

## Scenarios exercised by `exec.sh`

1. **Default apply.** `deacon templates apply` copies all files
   (including the optional ones) and substitutes `projectName` into
   the placeholders. This is the conservative default per spec —
   consumer tooling that doesn't prompt should include everything.
2. **Selective apply.** Run apply, then drop the three optional
   files and re-run `deacon templates apply --force`. The required
   files (devcontainer.json, README, setup.sh) get restored; the
   optional ones may stay absent or be restored depending on how the
   tool implements opt-out (deacon's CLI today implements default
   include-all; spec leaves selection to the IDE).

## Manual usage

```sh
DEST=$(mktemp -d)
deacon templates apply . --output "$DEST" \
	--option projectName=acme-svc

# Required files always present.
ls "$DEST/.devcontainer/devcontainer.json"
ls "$DEST/scripts/setup.sh"

# Optional files — present by default; an IDE prompt would let the user skip.
ls "$DEST/scripts/db-migrate.sh"
ls "$DEST/.github/workflows/ci.yml"
```

## Spec references

- `optionalPaths`:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-templates.md>
- Template distribution and substitution:
  <https://containers.dev/implementors/templates/>
