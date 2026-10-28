#!/bin/sh
# Portable Preview only. No installation, system registration or profile copying.
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export DOREMI_PREVIEW_DATA="${DOREMI_PREVIEW_DATA:-$HERE/doremi-preview-data}"
exec "$HERE/doremi-preview" "$@"
