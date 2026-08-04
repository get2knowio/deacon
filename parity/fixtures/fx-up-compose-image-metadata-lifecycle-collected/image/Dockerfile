# Fixture image for the image-metadata lifecycle COLLECTION case (#467), COMPOSE
# path.
#
# Same label content as `fx-up-image-metadata-lifecycle-collected`'s, under its
# own tag: the defect is path-independent (it lives in the config merge, not in
# either `up` path), and a single-path case would misattribute it. Two fixtures
# rather than one because the prepull step derives a fixture's local tag from its
# OWN config — a single-container fixture names it in devcontainer.json, a
# compose fixture in its compose file.
#
# Measured at oracle 0.87.0 before the fix, identically to the single-container
# twin: the reference wrote all seven lines, deacon only the three `ws-` ones.
# See the sibling fixture's Dockerfile for what each hook is separating.
#
# Built locally (never published) by `scripts/parity/prepull-fixture-images.sh`
# and the parity workflow's fixture-image step, to the tag the sibling COMPOSE
# file names — a compose fixture declares its base there, not in
# devcontainer.json.
FROM alpine:3.19
LABEL devcontainer.metadata='[{"onCreateCommand":"echo img1-onCreate >> /workspace/compose-lifecycle-collected.txt","postCreateCommand":"echo img1-postCreate >> /workspace/compose-lifecycle-collected.txt"},{"onCreateCommand":"echo img2-onCreate >> /workspace/compose-lifecycle-collected.txt","postStartCommand":"echo img2-postStart >> /workspace/compose-lifecycle-collected.txt"}]'
