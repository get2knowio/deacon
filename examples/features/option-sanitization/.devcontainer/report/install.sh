#!/bin/sh
set -e
mkdir -p /usr/local/share/option-sanitization
# Capture every env var whose name looks feature-derived (uppercase, may
# start with a digit if sanitization left one). The spec sanitizes
# option names by uppercasing letters, replacing non-word chars with
# `_`, and prefixing with the feature ID where applicable.
env | grep -E '^[A-Z][A-Z0-9_]*=' | sort > /usr/local/share/option-sanitization/all-env
{
	echo "MYSTRINGOPTION=${MYSTRINGOPTION-<unset>}"
	echo "MY_STRING_OPTION=${MY_STRING_OPTION-<unset>}"
	echo "ANOTHER_WEIRD_KEY=${ANOTHER_WEIRD_KEY-<unset>}"
	echo "FLAGOPTION=${FLAGOPTION-<unset>}"
} > /usr/local/share/option-sanitization/probes
