#!/bin/sh
set -e
# Install the entrypoint the feature declares. deacon chains feature
# entrypoints ahead of the container command, so this must end with `exec "$@"`.
mkdir -p /usr/local/share/contrib
cat > /usr/local/share/contrib/entrypoint.sh <<'EOF'
#!/bin/sh
# Marker proving the feature-contributed entrypoint actually ran at start.
echo "contrib entrypoint ran" > /tmp/contrib-entrypoint-ran
# The feature-contributed volume is mounted here; record into it too.
[ -d /contrib-data ] && echo started > /contrib-data/started 2>/dev/null || true
exec "$@"
EOF
chmod +x /usr/local/share/contrib/entrypoint.sh
