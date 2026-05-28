#!/bin/sh
set -e
# The spec mandates these env vars be set by the CLI when running a
# feature's install.sh. Dump them so the example can assert on the
# values after the container is up.
mkdir -p /usr/local/share/feature-env
{
	echo "_REMOTE_USER=${_REMOTE_USER-<unset>}"
	echo "_REMOTE_USER_HOME=${_REMOTE_USER_HOME-<unset>}"
	echo "_CONTAINER_USER=${_CONTAINER_USER-<unset>}"
	echo "_CONTAINER_USER_HOME=${_CONTAINER_USER_HOME-<unset>}"
} > /usr/local/share/feature-env/snapshot
