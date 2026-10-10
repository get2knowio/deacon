Authored for this suite (not vendored).

The service's command has an OBSERVABLE SIDE EFFECT, which is the whole point. Every
other Compose fixture in the suite declares `command: ["sleep", "infinity"]` — the one
idiom under which deacon's defect was invisible, because replacing that command with
deacon's own keep-alive is behaviourally identical. A fixture whose command WRITES
something can tell the two apart.

`workspaceFolder` is deliberately NOT declared: a Compose config without one resolves to
`/`, which always exists. Declaring `/workspace` while the service mounts nothing there is
the incoherent shape #460 removed from `smoke_compose_override_command`, where the
lifecycle hook then fails under `docker exec -w /workspace`.

See #749 and `bhv-up-compose-declared-command-runs`.
