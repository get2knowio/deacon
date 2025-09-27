# Lifecycle Commands Example

Shows all key lifecycle hooks (`initialize`, `onCreate`, `postCreate`, `postStart`, `postAttach`) and `waitFor` behavior.

Verify effects (after `deacon up .`):
- `/tmp/lifecycle_init` exists inside the container
- `/tmp/lifecycle_log` contains lines: onCreate, postCreate, postStart (and postAttach after attach)
- `/workspace/.deacon/status` contains `ready`

Run (from this directory):
```sh
deacon config validate .
```