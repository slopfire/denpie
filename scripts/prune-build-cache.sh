#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cache="$workspace/target"
limit_gib="${BUILD_CACHE_LIMIT_GIB:-32}"
limit_kib=$((limit_gib * 1024 * 1024))

[[ -d "$cache" ]] || exit 0

git_dir="$(git -C "$workspace" rev-parse --absolute-git-dir)"
exec 9>"$git_dir/cache-prune.lock"
flock -n 9 || exit 0

cache_size_kib() {
  du -sk -- "$cache" | cut -f1
}

compiler_is_active() {
  local proc_dir command process_cwd
  for proc_dir in /proc/[0-9]*; do
    [[ -r "$proc_dir/comm" ]] || continue
    read -r command <"$proc_dir/comm" || continue
    case "$command" in
      cargo|rustc|clippy-driver|sccache)
        process_cwd="$(readlink "$proc_dir/cwd" 2>/dev/null || true)"
        case "$process_cwd/" in
          "$workspace/"*) return 0 ;;
        esac
        ;;
    esac
  done
  return 1
}

cache_binary_is_active() {
  local proc_dir executable
  for proc_dir in /proc/[0-9]*; do
    executable="$(readlink "$proc_dir/exe" 2>/dev/null || true)"
    case "$executable" in
      "$cache/"*) return 0 ;;
    esac
  done
  return 1
}

size_kib="$(cache_size_kib)"
((size_kib > limit_kib)) || exit 0

if compiler_is_active; then
  printf 'Build cache is %s GiB, but Cargo is active in this repository. Skipping cleanup.\n' "$((size_kib / 1024 / 1024))" >&2
  exit 0
fi

printf 'Build cache is %s GiB. Removing Cargo incremental data.\n' "$((size_kib / 1024 / 1024))"
find "$cache" -type d -name incremental -prune -print0 | xargs -0 -r rm -rf --

size_kib="$(cache_size_kib)"
((size_kib > limit_kib)) || exit 0

if cache_binary_is_active; then
  printf 'Build cache remains %s GiB, but a binary from it is active. Keeping non-incremental artifacts.\n' "$((size_kib / 1024 / 1024))" >&2
  exit 0
fi

printf 'Build cache remains %s GiB. Running cargo clean.\n' "$((size_kib / 1024 / 1024))"
cargo clean --target-dir "$cache"
