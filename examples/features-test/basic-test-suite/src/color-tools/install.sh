#!/bin/bash
set -e

FAVORITE_COLOR="${FAVORITECOLOR:-blue}"

# Create a color configuration file
cat > /usr/local/etc/color-config.conf <<EOF
FAVORITE_COLOR=${FAVORITE_COLOR}
EOF

# Add environment variable
echo "export FAVORITE_COLOR='${FAVORITE_COLOR}'" >> /etc/bash.bashrc

echo "✓ color-tools feature installed successfully with color: ${FAVORITE_COLOR}"
