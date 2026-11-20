#!/bin/sh
# mdbook preprocessor entry point for the book's diagrams.
#
# mdbook first runs `<command> supports <renderer>` and silently skips the
# preprocessor if that exits non-zero. Running `cargo run` there would turn a
# compile error into a skipped preprocessor and a book full of raw
# {{#heavenly-diagram …}} directives, so answer `supports` here without
# building. Any failure then happens in the real pass, where mdbook aborts.
if [ "$1" = "supports" ]; then
  exit 0
fi
exec cargo run --quiet --locked -p heavenly-diagrams -- "$@"
