#!/bin/sh
set -e
cat > /usr/local/share/conf-ep-b.sh <<'INNER'
#!/bin/sh
echo conf-ep-b >> /workspace/entrypoint-chain.txt
INNER
chmod +x /usr/local/share/conf-ep-b.sh
