greet() {
  print -r -- "$GREET_WORD, $1"
}
compdef _greet hello-greet
