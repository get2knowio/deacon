# Fixture image for the same-phase lifecycle-hook collection case (#467).
#
# The spec's image-metadata Merge Logic table gives each of the five lifecycle
# hooks a "Collected list of all <phase>Commands", with "the devcontainer.json
# is considered last" — so when an image's `devcontainer.metadata` and the
# workspace's devcontainer.json declare the SAME phase, BOTH hooks run, the
# image's first. Every OTHER row of that table is last-wins, which is exactly
# how deacon folded the label in, so the image's hook was silently dropped on a
# collision. An image-only phase already worked (#448); the loss was specific to
# the collision — and a collision is the `mcr.microsoft.com/devcontainers/*`
# base-image shape, where the base declares a hook and the user declares another.
#
# Three phases, chosen so the case cannot pass for the wrong reason:
#   - `onCreateCommand`   — also declared by the sibling config (string vs
#     string): must COLLECT.
#   - `postCreateCommand` — also declared by the sibling config, in the ARRAY
#     form (string vs array): must collect regardless of command form, so the
#     collection is not quietly form-sensitive the way the compose post-create
#     once was (#460).
#   - `postStartCommand`  — declared HERE ONLY: must run exactly ONCE. This is
#     the half a naive fix breaks. Recording the hook as a collected layer while
#     the merge ALSO leaves it in the singular field runs it twice, and only an
#     assertion over the WHOLE log can see that; a `contains` check cannot.
#
# Every hook appends to one file, so the file records both WHICH hooks ran and
# the ORDER they ran in. Relative `./lifecycle.log` resolves against the
# container workspace folder, which both CLIs set for lifecycle hooks.
#
# No `remoteUser`: the hooks run as the image's root, so nothing about this case
# depends on the `updateRemoteUserUID` remap that `fx-up-compose-image-metadata`
# is built to exercise. This case measures the collection and nothing else.
#
# Built locally (never published) by `scripts/parity/prepull-fixture-images.sh`
# and the parity workflow's fixture-image step, to the `:local` tag the sibling
# devcontainer.json names. Both CLIs resolve the label by local `docker inspect`,
# so no registry is involved.
FROM alpine:3.19
LABEL devcontainer.metadata='[{"onCreateCommand":"echo img-onCreate >> ./lifecycle.log","postCreateCommand":"echo img-postCreate >> ./lifecycle.log","postStartCommand":"echo img-postStart >> ./lifecycle.log"}]'
