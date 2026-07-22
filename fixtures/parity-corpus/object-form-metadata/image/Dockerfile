# Fixture image for the `object-form-metadata` parity corpus case (#300/#332).
#
# Its `devcontainer.metadata` label is a single JSON OBJECT (the legacy form),
# not the JSON array the modern reference CLI bakes. `parity_corpus_merged`
# reads this label to prove deacon and the pinned oracle parse the object form
# identically — coverage that no public image provides (they are all array-form).
#
# Built locally (never published) by the parity CI lane's "Build corpus fixture
# images" step to the tag the fixture's devcontainer.json references. Both CLIs
# resolve the label by local `docker inspect`, so no registry is involved.
FROM alpine:3.19
LABEL devcontainer.metadata='{"customizations":{"vscode":{"extensions":["deacon.object-form"]}},"remoteUser":"objuser","containerEnv":{"OBJ_FORM":"yes"}}'
