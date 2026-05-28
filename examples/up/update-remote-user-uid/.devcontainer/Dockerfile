FROM alpine:3.18

# Create a user with a deliberately weird UID/GID so the sync step has
# something visible to change.
RUN addgroup -g 5000 devuser \
	&& adduser -D -u 5000 -G devuser devuser

USER devuser
