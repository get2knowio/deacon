#!/bin/sh
set -e

echo "Activating feature 'localFeatureA'"

GREETING=${GREETING:-undefined}
echo "The provided greeting is: $GREETING"
