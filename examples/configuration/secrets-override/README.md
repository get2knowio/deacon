# Override Config + Secrets Example

Shows how to merge an override config and substitute secrets from a `.env`-style file.

Usage (from this directory):
```sh
# Inspect merged configuration (override takes precedence; secrets substituted)
deacon --config devcontainer.jsonc \
  --override-config override.jsonc \
  --secrets-file secrets.env \
  read-configuration
```

Notes:
- Secrets file format is KEY=VALUE with `#` comments supported.
- Later `--secrets-file` instances override earlier ones on key conflicts.