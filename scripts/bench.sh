#!/usr/bin/env bash
# Compares interactive zsh start-up time under Oh My Zsh and zsh-tool with
# the same plugins, each in an isolated home directory.
#
# Usage: scripts/bench.sh <oh-my-zsh checkout> [plugin...]
# Requires zsh and hyperfine; builds zsh-tool in release mode.
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "usage: $0 <oh-my-zsh checkout> [plugin...]" >&2
    exit 2
fi
omz=$(cd "$1" && pwd)
shift
plugins=("$@")
if [[ ${#plugins[@]} -eq 0 ]]; then
    plugins=(git sudo z extract colored-man-pages history docker kubectl npm python)
fi

root=$(cd "$(dirname "$0")/.." && pwd)
cargo build --release --quiet --manifest-path "$root/Cargo.toml"
bin="$root/target/release/zsh-tool"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Oh My Zsh: the standard .zshrc layout.
mkdir -p "$work/omz"
cat >"$work/omz/.zshrc" <<ZSHRC
export ZSH='$omz'
ZSH_THEME=""
DISABLE_AUTO_UPDATE=true
plugins=(${plugins[*]})
source \$ZSH/oh-my-zsh.sh
ZSHRC

# zsh-tool: same plugins, resolved from the same checkout.
mkdir -p "$work/zt/.config/zsh-tool"
{
    printf 'plugins = ['
    printf '"%s", ' "${plugins[@]}"
    printf ']\n'
} >"$work/zt/.config/zsh-tool/config.toml"
cat >"$work/zt/.zshrc" <<ZSHRC
export ZSH='$omz'
eval "\$('$bin' init zsh)"
ZSHRC

# Warm-up runs fill the caches (compinit dump, zsh-tool init script).
hyperfine --warmup 3 --runs "${RUNS:-30}" --shell=none \
    --export-markdown "${EXPORT:-/dev/stdout}" \
    -n "oh-my-zsh (${#plugins[@]} plugins)" "env -i HOME=$work/omz TERM=dumb zsh -i -c exit" \
    -n "zsh-tool (${#plugins[@]} plugins)" "env -i HOME=$work/zt TERM=dumb zsh -i -c exit" \
    -n "zsh-tool init zsh (cache hit)" "env -i HOME=$work/zt ZSH=$omz $bin init zsh"
