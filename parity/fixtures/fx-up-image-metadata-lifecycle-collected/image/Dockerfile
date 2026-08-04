# Fixture image for the image-metadata lifecycle COLLECTION case (#467), single
# container path.
#
# The label carries TWO entries, and between them they cover the three shapes the
# case has to separate:
#
#   - `onCreateCommand` in BOTH entries AND in the sibling devcontainer.json —
#     the collision the bug was about, and the one that also pins order ACROSS
#     metadata entries (img1 before img2 before the workspace's).
#   - `postCreateCommand` in entry 1 and in devcontainer.json — the same
#     collision against the ARRAY command form on the workspace side.
#   - `postStartCommand` in entry 2 and in devcontainer.json — the same collision
#     against the OBJECT form.
#
# Every hook appends to one file, so the file IS the ordering assertion. That is
# safe for the object-form phase because its object declares a single named
# command: the spec orders the metadata ENTRIES (devcontainer.json last) but says
# nothing about the order of names WITHIN an object, so a second name here would
# pin an order neither CLI promises.
#
# Measured at oracle 0.87.0 before the fix: the reference wrote all seven lines
# and deacon wrote only the three `ws-` ones — the image's hooks had been
# OVERWRITTEN by the workspace's, because `ConfigMerger` treated a lifecycle
# command as an ordinary last-value-wins scalar. The spec's image-metadata merge
# table gives every lifecycle hook the merge logic "Collected list of all
# <phase>Commands", with "the devcontainer.json is considered last" when order
# matters.
#
# Built locally (never published) by `scripts/parity/prepull-fixture-images.sh`
# and the parity workflow's fixture-image step, to the tag the sibling
# devcontainer.json names. Both CLIs resolve the label by local `docker inspect`,
# so no registry is involved.
FROM alpine:3.19
LABEL devcontainer.metadata='[{"onCreateCommand":"echo img1-onCreate >> /workspace/lifecycle-collected.txt","postCreateCommand":"echo img1-postCreate >> /workspace/lifecycle-collected.txt"},{"onCreateCommand":"echo img2-onCreate >> /workspace/lifecycle-collected.txt","postStartCommand":"echo img2-postStart >> /workspace/lifecycle-collected.txt"}]'
