# Oh My Zsh theme API, implemented by zsh-tool so that *.zsh-theme files
# work without Oh My Zsh's lib/ directory. Defined before the plugins, so a
# plugin (or lib/ with compat.lib) providing a real implementation wins.

autoload -Uz colors && colors
setopt prompt_subst

# Theme settings and their Oh My Zsh defaults.
ZSH_THEME_GIT_PROMPT_PREFIX="git:("
ZSH_THEME_GIT_PROMPT_SUFFIX=")"
ZSH_THEME_GIT_PROMPT_DIRTY="*"
ZSH_THEME_GIT_PROMPT_CLEAN=""
ZSH_THEME_RUBY_PROMPT_PREFIX="("
ZSH_THEME_RUBY_PROMPT_SUFFIX=")"
ZSH_THEME_RVM_PROMPT_OPTIONS="i v g"
ZSH_THEME_TERM_TAB_TITLE_IDLE="%15<..<%~%<<"
ZSH_THEME_TERM_TITLE_IDLE="%n@%m:%~"
[[ $TERM_PROGRAM != Apple_Terminal ]] || ZSH_THEME_TERM_TITLE_IDLE="%n@%m"

# Prompt git calls are read-only: they must not take the optional locks
# that a git command run by the user may need.
function __git_prompt_git() {
  GIT_OPTIONAL_LOCKS=0 command git "$@"
}

# Branch (or tag, or short SHA) wrapped in the theme prefix and suffix,
# followed by the dirty marker; nothing outside a repository.
function git_prompt_info() {
  local -a info
  # One call gives the git dir and the branch; on an unborn branch git
  # prints the git dir and then fails.
  info=("${(@f)$(__git_prompt_git rev-parse --git-dir --abbrev-ref HEAD 2>/dev/null)}")
  local unborn=$(( $? != 0 ))
  [[ -n $info[1] ]] || return 0
  [[ "$(__git_prompt_git config --get oh-my-zsh.hide-info 2>/dev/null)" != 1 ]] || return 0

  local ref=$info[-1]
  if (( unborn )); then
    ref=$(__git_prompt_git symbolic-ref --short HEAD 2>/dev/null) || return 0
  elif [[ $ref == HEAD ]]; then
    ref=$(__git_prompt_git describe --tags --exact-match HEAD 2>/dev/null) \
      || ref=$(__git_prompt_git rev-parse --short HEAD 2>/dev/null) \
      || return 0
  fi

  local upstream
  if (( ${+ZSH_THEME_GIT_SHOW_UPSTREAM} )); then
    upstream=$(__git_prompt_git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null) \
      && upstream=" -> $upstream"
  fi
  echo "${ZSH_THEME_GIT_PROMPT_PREFIX}${ref//\%/%%}${upstream//\%/%%}$(parse_git_dirty)${ZSH_THEME_GIT_PROMPT_SUFFIX}"
}

# ZSH_THEME_GIT_PROMPT_DIRTY or _CLEAN depending on the working tree.
function parse_git_dirty() {
  local line
  local -a flags=(--porcelain)
  if [[ "$(__git_prompt_git config --get oh-my-zsh.hide-dirty 2>/dev/null)" != 1 ]]; then
    [[ ${DISABLE_UNTRACKED_FILES_DIRTY:-} != true ]] || flags+=(--untracked-files=no)
    # "git" lets git apply the repository settings; unset means "dirty".
    [[ ${GIT_STATUS_IGNORE_SUBMODULES:-} == git ]] \
      || flags+=(--ignore-submodules=${GIT_STATUS_IGNORE_SUBMODULES:-dirty})
    __git_prompt_git status $flags 2>/dev/null | read -r line
  fi
  if [[ -n $line ]]; then
    echo "$ZSH_THEME_GIT_PROMPT_DIRTY"
  else
    echo "$ZSH_THEME_GIT_PROMPT_CLEAN"
  fi
}

# One ZSH_THEME_GIT_PROMPT_<STATE> marker per state found in the repository.
function git_prompt_status() {
  setopt localoptions extendedglob
  [[ "$(__git_prompt_git config --get oh-my-zsh.hide-status 2>/dev/null)" != 1 ]] || return 0

  local text
  text=$(__git_prompt_git status --porcelain -b 2>/dev/null)
  (( $? != 128 )) || return 1

  local -A seen
  __git_prompt_git rev-parse --verify refs/stash &>/dev/null && seen[STASHED]=1

  local line part
  local -a lines=("${(@f)text}")
  # "## main...origin/main [ahead 1, behind 2]"
  if [[ $lines[1] == (#b)'## '[^\ ]##' ['(*)']' ]]; then
    for part in ${(s:,:)match[1]}; do
      case ${part## #} in
        ahead*) seen[AHEAD]=1 ;;
        behind*) seen[BEHIND]=1 ;;
        diverged*) seen[DIVERGED]=1 ;;
      esac
    done
  fi
  for line in $lines; do
    case ${line[1,3]} in
      '?? ') seen[UNTRACKED]=1 ;;
      'A  ') seen[ADDED]=1 ;;
      'M  '|'MM '|' M '|'AM '|' T ') seen[MODIFIED]=1 ;;
      'R  ') seen[RENAMED]=1 ;;
      ' D '|'D  ') seen[DELETED]=1 ;;
      'UU ') seen[UNMERGED]=1 ;;
    esac
  done

  # Each marker is prepended, so the last state of the list comes first.
  local state markers marker
  for state in UNTRACKED ADDED MODIFIED RENAMED DELETED STASHED UNMERGED AHEAD BEHIND DIVERGED; do
    if (( ${+seen[$state]} )); then
      marker=ZSH_THEME_GIT_PROMPT_$state
      markers="${(P)marker}$markers"
    fi
  done
  echo $markers
}

# Position of the branch against its upstream.
function git_remote_status() {
  local remote ahead behind remote_status detailed
  remote=${$(__git_prompt_git rev-parse --verify '@{upstream}' --symbolic-full-name 2>/dev/null)/refs\/remotes\/}
  [[ -n $remote ]] || return 0
  ahead=$(__git_prompt_git rev-list '@{upstream}..HEAD' 2>/dev/null | wc -l)
  behind=$(__git_prompt_git rev-list 'HEAD..@{upstream}' 2>/dev/null | wc -l)

  local ahead_part="$ZSH_THEME_GIT_PROMPT_AHEAD_REMOTE_COLOR$ZSH_THEME_GIT_PROMPT_AHEAD_REMOTE$((ahead))%{$reset_color%}"
  local behind_part="$ZSH_THEME_GIT_PROMPT_BEHIND_REMOTE_COLOR$ZSH_THEME_GIT_PROMPT_BEHIND_REMOTE$((behind))%{$reset_color%}"
  if (( ahead == 0 && behind == 0 )); then
    remote_status=$ZSH_THEME_GIT_PROMPT_EQUAL_REMOTE
  elif (( behind == 0 )); then
    remote_status=$ZSH_THEME_GIT_PROMPT_AHEAD_REMOTE
    detailed=$ahead_part
  elif (( ahead == 0 )); then
    remote_status=$ZSH_THEME_GIT_PROMPT_BEHIND_REMOTE
    detailed=$behind_part
  else
    remote_status=$ZSH_THEME_GIT_PROMPT_DIVERGED_REMOTE
    detailed=$ahead_part$behind_part
  fi
  if [[ -n $ZSH_THEME_GIT_PROMPT_REMOTE_STATUS_DETAILED ]]; then
    remote_status="$ZSH_THEME_GIT_PROMPT_REMOTE_STATUS_PREFIX${remote//\%/%%}$detailed$ZSH_THEME_GIT_PROMPT_REMOTE_STATUS_SUFFIX"
  fi
  echo $remote_status
}

# Current branch name, or short SHA on a detached HEAD.
function git_current_branch() {
  local ref
  ref=$(__git_prompt_git symbolic-ref --quiet HEAD 2>/dev/null)
  local ret=$?
  if (( ret != 0 )); then
    (( ret != 128 )) || return 0
    ref=$(__git_prompt_git rev-parse --short HEAD 2>/dev/null) || return 0
  fi
  echo ${ref#refs/heads/}
}

# Previously checked out branch, if it was a branch.
function git_previous_branch() {
  local ref
  ref=$(__git_prompt_git rev-parse --quiet --symbolic-full-name '@{-1}' 2>/dev/null) || return 0
  [[ -n $ref ]] && echo ${ref#refs/heads/}
}

function git_commits_ahead() {
  __git_prompt_git rev-parse --git-dir &>/dev/null || return 0
  local commits=$(__git_prompt_git rev-list --count '@{upstream}..HEAD' 2>/dev/null)
  [[ -z $commits || $commits == 0 ]] \
    || echo "$ZSH_THEME_GIT_COMMITS_AHEAD_PREFIX$commits$ZSH_THEME_GIT_COMMITS_AHEAD_SUFFIX"
}

function git_commits_behind() {
  __git_prompt_git rev-parse --git-dir &>/dev/null || return 0
  local commits=$(__git_prompt_git rev-list --count 'HEAD..@{upstream}' 2>/dev/null)
  [[ -z $commits || $commits == 0 ]] \
    || echo "$ZSH_THEME_GIT_COMMITS_BEHIND_PREFIX$commits$ZSH_THEME_GIT_COMMITS_BEHIND_SUFFIX"
}

function git_prompt_ahead() {
  [[ -z "$(__git_prompt_git rev-list origin/$(git_current_branch)..HEAD 2>/dev/null)" ]] \
    || echo "$ZSH_THEME_GIT_PROMPT_AHEAD"
}

function git_prompt_behind() {
  [[ -z "$(__git_prompt_git rev-list HEAD..origin/$(git_current_branch) 2>/dev/null)" ]] \
    || echo "$ZSH_THEME_GIT_PROMPT_BEHIND"
}

function git_prompt_remote() {
  if [[ -n "$(__git_prompt_git show-ref origin/$(git_current_branch) 2>/dev/null)" ]]; then
    echo "$ZSH_THEME_GIT_PROMPT_REMOTE_EXISTS"
  else
    echo "$ZSH_THEME_GIT_PROMPT_REMOTE_MISSING"
  fi
}

function git_prompt_short_sha() {
  local sha
  sha=$(__git_prompt_git rev-parse --short HEAD 2>/dev/null) \
    && echo "$ZSH_THEME_GIT_PROMPT_SHA_BEFORE$sha$ZSH_THEME_GIT_PROMPT_SHA_AFTER"
}

function git_prompt_long_sha() {
  local sha
  sha=$(__git_prompt_git rev-parse HEAD 2>/dev/null) \
    && echo "$ZSH_THEME_GIT_PROMPT_SHA_BEFORE$sha$ZSH_THEME_GIT_PROMPT_SHA_AFTER"
}

function git_current_user_name() {
  __git_prompt_git config user.name 2>/dev/null
}

function git_current_user_email() {
  __git_prompt_git config user.email 2>/dev/null
}

function git_repo_name() {
  local top
  top=$(__git_prompt_git rev-parse --show-toplevel 2>/dev/null) && [[ -n $top ]] && echo ${top:t}
}

# Placeholders for prompt functions that plugins provide: without the
# plugin they print nothing, so themes can call them unconditionally.
function azure_prompt_info bzr_prompt_info chruby_prompt_info conda_prompt_info \
  go_prompt_info hg_prompt_info jenv_prompt_info nvm_prompt_info pyenv_prompt_info \
  rbenv_prompt_info svn_prompt_info tf_prompt_info vi_mode_prompt_info \
  virtualenv_prompt_info {
  return 1
}

# Ruby version from rvm, when rvm is installed.
function rvm_prompt_info() {
  [[ -f $HOME/.rvm/bin/rvm-prompt ]] || return 1
  local version
  version=$($HOME/.rvm/bin/rvm-prompt ${=ZSH_THEME_RVM_PROMPT_OPTIONS} 2>/dev/null)
  [[ -n $version ]] || return 1
  echo "${ZSH_THEME_RUBY_PROMPT_PREFIX}${version:gs/%/%%}${ZSH_THEME_RUBY_PROMPT_SUFFIX}"
}

function ruby_prompt_info() {
  echo "$(rvm_prompt_info || rbenv_prompt_info || chruby_prompt_info)"
}

# Colored ls and diff. Each probe forks a command, so its answer is kept
# for a day in $ZSH_CACHE_DIR.
typeset -gA _zsh_tool_probes
() {
  local cache=$ZSH_CACHE_DIR/zsh-tool-probes
  local -a fresh=("$cache"(Nm-1))
  (( ! $#fresh )) || source "$cache"
}

function _zsh_tool_probe() {
  local key="$*"
  if (( ! ${+_zsh_tool_probes[$key]} )); then
    command "$@" /dev/null &>/dev/null
    _zsh_tool_probes[$key]=$?
    [[ ! -w $ZSH_CACHE_DIR ]] \
      || print -r -- "_zsh_tool_probes=(" "${(@qqkv)_zsh_tool_probes}" ")" >| "$ZSH_CACHE_DIR/zsh-tool-probes"
  fi
  return $_zsh_tool_probes[$key]
}

if _zsh_tool_probe diff --color /dev/null; then
  function diff {
    command diff --color "$@"
  }
fi

() {
  [[ $DISABLE_LS_COLORS != true ]] || return 0

  # BSD ls
  export LSCOLORS="Gxfxcxdxbxegedabagacad"
  # GNU ls
  if [[ -z $LS_COLORS ]]; then
    if (( $+commands[dircolors] )); then
      if [[ -f $HOME/.dircolors ]]; then
        source <(dircolors -b "$HOME/.dircolors")
      else
        source <(dircolors -b)
      fi
    else
      export LS_COLORS="di=1;36:ln=35:so=32:pi=33:ex=31:bd=34;46:cd=34;43:su=30;41:sg=30;46:tw=30;42:ow=30;43"
    fi
  fi

  case $OSTYPE in
    netbsd*)
      _zsh_tool_probe gls --color && alias ls='gls --color=tty'
      ;;
    openbsd*)
      _zsh_tool_probe gls --color && alias ls='gls --color=tty'
      _zsh_tool_probe colorls -G && alias ls='colorls -G'
      ;;
    (darwin|freebsd)*)
      _zsh_tool_probe ls -G && alias ls='ls -G'
      zstyle -t ':omz:lib:theme-and-appearance' gnu-ls \
        && _zsh_tool_probe gls --color \
        && alias ls='gls --color=tty'
      ;;
    *)
      if _zsh_tool_probe ls --color; then
        alias ls='ls --color=tty'
      elif _zsh_tool_probe ls -G; then
        alias ls='ls -G'
      fi
      ;;
  esac
  return 0
}

unfunction _zsh_tool_probe
unset _zsh_tool_probes
