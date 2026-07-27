#!/bin/sh
set -e
addgroup -g 4343 featuregroup
adduser -D -u 4343 -G featuregroup featureuser
