#!/bin/sh
set -e
cat > /usr/local/share/conf-ep-a.sh <<'INNER'
#!/bin/sh
echo conf-ep-a >> /workspace/entrypoint-chain.txt
INNER
chmod +x /usr/local/share/conf-ep-a.sh
