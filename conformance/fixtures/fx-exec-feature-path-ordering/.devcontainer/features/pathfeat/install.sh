#!/bin/sh
set -e
mkdir -p /opt/feature/bin /etc/profile.d
printf '#!/bin/sh\necho feature-tool\n' > /opt/feature/bin/feature-tool
chmod +x /opt/feature/bin/feature-tool
echo 'export PATH=/opt/feature/bin:$PATH' > /etc/profile.d/pathfeat.sh
