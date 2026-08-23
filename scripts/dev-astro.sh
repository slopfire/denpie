#!/usr/bin/env sh
# Alias of scripts/dev.sh (Astro is the only frontend).
exec "$(CDPATH= cd -- "$(dirname "$0")" && pwd)/dev.sh"
