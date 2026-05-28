#!/bin/sh
set -e
# Real install would set up the monitor daemon; here we just record presence.
mkdir -p /usr/local/share/monitor
echo "installed" > /usr/local/share/monitor/installed
