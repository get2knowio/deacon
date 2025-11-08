#!/bin/bash
set -e

# Read the greeting option (defaults to "Hello")
GREETING="${GREETING:-Hello}"

echo "${GREETING}, World!" > /usr/local/hello-message.txt
echo "export HELLO_MESSAGE='${GREETING}, World!'" >> /etc/bash.bashrc

echo "✓ hello-world feature installed successfully"
