#!/bin/sh
set -eu
exec python3 "$(dirname "$0")/gen-surface-matrix.py" "$@"
