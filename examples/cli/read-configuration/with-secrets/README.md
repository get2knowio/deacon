# Secrets Management Example

This example demonstrates using `--secrets-file` for secure variable substitution.

## Usage

```bash
deacon read-configuration --workspace-folder . \
  --config devcontainer.json \
  --secrets-file secrets.env
```

## Expected Output

Variables like `${localEnv:DATABASE_URL}` will be substituted with values from `secrets.env`.

By default, secrets are redacted in the output for security.

## Disable Redaction (Debug Only)

```bash
deacon read-configuration --workspace-folder . \
  --config devcontainer.json \
  --secrets-file secrets.env \
  --no-redact
```

**WARNING**: Only use `--no-redact` in secure, isolated environments for debugging. Never use in production or shared outputs.

## What It Demonstrates

- Loading secrets from file
- Variable substitution from secrets
- Secret redaction in output
- Debug mode to view actual values

## Security Best Practices

1. **Never commit secrets files to version control**
   - Add to `.gitignore`: `*.env`, `secrets.*`

2. **Use environment-specific secrets files**
   - `secrets.dev.env`, `secrets.prod.env`

3. **Restrict file permissions**
   ```bash
   chmod 600 secrets.env
   ```

4. **Rotate secrets regularly**

5. **Use secret management systems in production**
   - AWS Secrets Manager
   - HashiCorp Vault
   - Azure Key Vault
