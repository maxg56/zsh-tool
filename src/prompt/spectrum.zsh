# 256-color tables used by themes: ${FG[042]}, ${BG[042]}, ${FX[bold]}.
typeset -AHg FX FG BG
() {
  setopt localoptions noksharrays
  local esc=$'\e'
  FX=(
    reset "%{${esc}[00m%}"
    bold "%{${esc}[01m%}" no-bold "%{${esc}[22m%}"
    dim "%{${esc}[02m%}" no-dim "%{${esc}[22m%}"
    italic "%{${esc}[03m%}" no-italic "%{${esc}[23m%}"
    underline "%{${esc}[04m%}" no-underline "%{${esc}[24m%}"
    blink "%{${esc}[05m%}" no-blink "%{${esc}[25m%}"
    reverse "%{${esc}[07m%}" no-reverse "%{${esc}[27m%}"
  )
  local -a codes=({000..255})
  local -a fg=("%{${esc}[38;5;"${^codes}"m%}") bg=("%{${esc}[48;5;"${^codes}"m%}")
  FG=(${codes:^fg})
  BG=(${codes:^bg})
}
