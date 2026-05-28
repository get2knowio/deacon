# Features: Pin a Feature by OCI Digest (`@sha256:...`)

OCI references can be pinned in three ways, in increasing strictness:

| Ref form                                  | Reproducible? | Notes                          |
|-------------------------------------------|---------------|--------------------------------|
| `ghcr.io/.../features/git:1`              | No (floats)   | Picks newest tag matching `1`. |
| `ghcr.io/.../features/git:1.3.0`          | Mostly        | Until a republish overwrites.  |
| `ghcr.io/.../features/git:1@sha256:abc…`  | Fully         | Spec-recommended for CI/locks. |

This example pins the `git` feature by digest. Re-running `deacon up`
will fetch exactly the same bytes every time, even if the floating tag
moves underneath you. This is the form `devcontainer-lock.json` uses to
freeze installs for reproducibility.

## Files

- `devcontainer.json` — declares `features` with a digest-pinned ref.
  The placeholder digest in this file (`sha256:REPLACE_WITH_REAL_DIGEST`)
  must be resolved before running; `exec.sh` does that on demand by
  looking up the current digest from the registry.

## Scenarios exercised by `exec.sh`

1. **Resolve the digest.** Query the registry for `git:1`'s manifest
   digest with `docker manifest inspect` (or skopeo), then patch
   `devcontainer.json` in place with the real value.
2. **`read-configuration --include-features-configuration`** shows the
   pinned ref intact in the parsed config.
3. **`deacon up`** pulls the feature by digest and installs it. The
   feature ref recorded in `devcontainer-lock.json` (if generated)
   matches.

## Manual usage

```sh
# 1. Discover the current digest for tag 1 (one-shot).
DIGEST=$(docker manifest inspect ghcr.io/devcontainers/features/git:1 \
	--verbose 2>/dev/null | jq -r '.[0].Descriptor.digest')

# 2. Patch the config to pin it.
sed -i "s|@sha256:REPLACE_WITH_REAL_DIGEST|@${DIGEST}|" devcontainer.json

# 3. Up + verify.
deacon up --workspace-folder . --remove-existing-container
```

## Notes

- This example requires network access to ghcr.io. CI without registry
  egress should skip it.
- If `docker manifest inspect` is unavailable, fall back to
  `skopeo inspect docker://ghcr.io/devcontainers/features/git:1`.

## Spec references

- Feature reference + digest pinning:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-features-distribution.md>
- Lockfile contract:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-lockfile.md>
