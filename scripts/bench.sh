#!/usr/bin/env bash
# Compares interactive zsh start-up time under Oh My Zsh and zsh-tool with
# the same plugins, each in an isolated home directory, or checks that each
# plugin loads without error.
#
# Usage: scripts/bench.sh [--native <plugins dir>] [--check] [--set 10|20]
#                         <oh-my-zsh checkout> [plugin...]
#
#   --native <dir>  also run zsh-tool with the native plugins of <dir> (the
#                   `plugins` directory of zsh-tool-plugins), placed in
#                   $ZSH_TOOL_HOME/plugins; plugins it lacks fall back to
#                   Oh My Zsh's.
#   --check         instead of timing, start `zsh -i -c exit` twice (cold,
#                   then cached) with each plugin alone, under Oh My Zsh (for
#                   reference), zsh-tool with the Oh My Zsh plugin (legacy)
#                   and zsh-tool with the native plugin when there is one.
#                   A run fails when it writes on stderr or exits non-zero.
#                   Exits non-zero when any zsh-tool run fails.
#   --set 10|20     plugin list used when none is given: the 10 plugins of
#                   the start-up target (default when timing) or the 20
#                   reference plugins (default with --check).
#
# Environment: RUNS (default 30) and EXPORT (markdown output, default
# stdout) for hyperfine; PRELUDE, zsh code put at the top of every .zshrc
# (e.g. zstyle settings). PATH, NVM_DIR and PYENV_ROOT, when set, are passed
# to the measured shells; everything else is cleared.
# Requires zsh and hyperfine (not needed with --check); builds zsh-tool in
# release mode.
set -euo pipefail

ten=(git sudo z extract colored-man-pages history docker kubectl npm python)
twenty=("${ten[@]}" docker-compose node nvm pip pyenv fzf command-not-found
    web-search copypath dirhistory)

usage() {
    echo "usage: $0 [--native <plugins dir>] [--check] [--set 10|20] <oh-my-zsh checkout> [plugin...]" >&2
    exit 2
}

native= check= set=
while [[ $# -gt 0 ]]; do
    case $1 in
    --native)
        [[ $# -ge 2 ]] || usage
        native=$(cd "$2" && pwd)
        shift 2
        ;;
    --check)
        check=1
        shift
        ;;
    --set)
        [[ $# -ge 2 && ($2 == 10 || $2 == 20) ]] || usage
        set=$2
        shift 2
        ;;
    -*) usage ;;
    *) break ;;
    esac
done
[[ $# -ge 1 ]] || usage
omz=$(cd "$1" && pwd)
shift
plugins=("$@")
if [[ ${#plugins[@]} -eq 0 ]]; then
    case ${set:-${check:+20}} in
    20) plugins=("${twenty[@]}") ;;
    *) plugins=("${ten[@]}") ;;
    esac
fi

root=$(cd "$(dirname "$0")/.." && pwd)
cargo build --release --quiet --manifest-path "$root/Cargo.toml"
bin="$root/target/release/zsh-tool"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Environment of the measured shells.
shell_env=(env -i "HOME=@HOME@" TERM=dumb)
for var in PATH NVM_DIR PYENV_ROOT; do
    if [[ -n ${!var:-} ]]; then
        shell_env+=("$var=${!var}")
    fi
done

# make_home <kind> <home> <plugin...>: writes a .zshrc for kind omz,
# legacy (zsh-tool with Oh My Zsh's plugins) or native.
make_home() {
    local kind=$1 home=$2
    shift 2
    mkdir -p "$home"
    {
        printf '%s\n' "${PRELUDE:-}"
        printf "export ZSH='%s'\n" "$omz"
        case $kind in
        omz)
            printf 'ZSH_THEME=""\nDISABLE_AUTO_UPDATE=true\n'
            printf 'plugins=(%s)\n' "$*"
            printf 'source $ZSH/oh-my-zsh.sh\n'
            ;;
        *)
            if [[ $kind == native ]]; then
                mkdir -p "$home/zsh-tool-home"
                ln -s "$native" "$home/zsh-tool-home/plugins"
                printf "export ZSH_TOOL_HOME='%s'\n" "$home/zsh-tool-home"
            fi
            printf "eval \"\$('%s' init zsh)\"\n" "$bin"
            mkdir -p "$home/.config/zsh-tool"
            {
                printf 'plugins = ['
                printf '"%s", ' "$@"
                printf ']\n'
            } >"$home/.config/zsh-tool/config.toml"
            ;;
        esac
    } >"$home/.zshrc"
}

# run_shell <home>: one `zsh -i -c exit` in that home.
run_shell() {
    "${shell_env[@]/@HOME@/$1}" zsh -i -c exit
}

if [[ -n $check ]]; then
    kinds=(omz legacy)
    [[ -n $native ]] && kinds+=(native)
    # check_one <kind> <plugin>: prints ok, FAIL or "-" (no native port).
    check_one() {
        local kind=$1 plugin=$2 home="$work/$1-$2" run
        if [[ $kind == native && ! -f $native/$plugin/plugin.toml ]]; then
            echo -
            return
        fi
        make_home "$kind" "$home" "$plugin"
        for run in cold cached; do
            if ! run_shell "$home" >"$home/stdout" 2>"$home/stderr" || [[ -s $home/stderr ]]; then
                echo FAIL
                {
                    echo "$plugin ($kind, $run start):"
                    sed 's/^/    /' "$home/stderr"
                } >>"$work/failures"
                return
            fi
        done
        echo ok
    }

    header="| Plugin |" rule="|:---|"
    for kind in "${kinds[@]}"; do
        header+=" $kind |" rule+=":---:|"
    done
    printf '%s\n%s\n' "$header" "$rule"
    declare -A passed=()
    effective=0
    for plugin in "${plugins[@]}"; do
        row="| $plugin |"
        for kind in "${kinds[@]}"; do
            result=$(check_one "$kind" "$plugin")
            row+=" $result |"
            [[ $result == ok ]] && passed[$kind]=$((${passed[$kind]:-0} + 1))
            [[ $kind == native ]] && native_result=$result
            [[ $kind == legacy ]] && legacy_result=$result
        done
        # What a zsh-tool user gets: the native port when there is one.
        result=$legacy_result
        [[ -n $native && $native_result != - ]] && result=$native_result
        [[ $result == ok ]] && effective=$((effective + 1))
        echo "$row"
    done
    total=${#plugins[@]}
    echo
    for kind in "${kinds[@]}"; do
        echo "$kind: ${passed[$kind]:-0}/$total"
    done
    echo "zsh-tool (native when available): $effective/$total"
    if [[ -s $work/failures ]]; then
        echo
        echo "Failures:"
        cat "$work/failures"
    fi
    status=0
    for kind in legacy native; do
        grep -q "($kind," "$work/failures" 2>/dev/null && status=1
    done
    exit $status
fi

make_home omz "$work/omz" "${plugins[@]}"
make_home legacy "$work/zt" "${plugins[@]}"
commands=(
    -n "oh-my-zsh (${#plugins[@]} plugins)" "${shell_env[*]/@HOME@/$work/omz} zsh -i -c exit"
    -n "zsh-tool legacy (${#plugins[@]} plugins)" "${shell_env[*]/@HOME@/$work/zt} zsh -i -c exit"
)
if [[ -n $native ]]; then
    make_home native "$work/zt-native" "${plugins[@]}"
    commands+=(-n "zsh-tool native (${#plugins[@]} plugins)"
        "${shell_env[*]/@HOME@/$work/zt-native} zsh -i -c exit")
fi
commands+=(-n "zsh-tool init zsh (cache hit)" "env -i HOME=$work/zt ZSH=$omz $bin init zsh")

# Warm-up runs fill the caches (compinit dump, zsh-tool init script).
hyperfine --warmup 3 --runs "${RUNS:-30}" --shell=none \
    --export-markdown "${EXPORT:-/dev/stdout}" "${commands[@]}"
