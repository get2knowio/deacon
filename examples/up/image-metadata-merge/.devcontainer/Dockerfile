FROM alpine:3.18

# Bake DevContainer metadata into the image as the spec-mandated label.
# The CLI is required to read this label and merge its contents with the
# user's devcontainer.json (image metadata is the lower-precedence layer).
LABEL devcontainer.metadata='[{ \
	"remoteUser": "root", \
	"containerEnv": { \
		"IMAGE_LAYER": "from-image-label", \
		"MERGED_LAYER": "image-wins" \
	} \
}]'

# Independently provide an OCI-style label for visibility.
LABEL org.opencontainers.image.title="image-metadata-merge-fixture"
