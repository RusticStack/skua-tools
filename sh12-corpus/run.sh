#!/bin/bash
# M3-SH0 corpus runner. Run it from Git Bash: `bash -o igncr run.sh ...` (igncr: CRLF checkouts).
#
#   run.sh --record                         record expected/*.txt from Git Bash
#   run.sh --shell gitbash                  re-run Git Bash against expected (determinism check)
#   run.sh --shell <path\to\brush.exe> --uutils <dir> [--extra-path <dir>] [--filter <regex>]
#
# Every case runs in its own fresh sandbox under $SKUA_SH0_TMP (default: $TMP), created by
# fixture.sh (always run by Git Bash, so both shells see the same files and git hashes). Only
# stdout and the exit code are compared, after dropping CR. The sandbox root is deleted at the end
# unless --keep. Cases are harmless: no network, no writes outside the sandbox, no git push.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
MODE=compare SHELL_UNDER_TEST=gitbash UUTILS="" EXTRA="" FILTER="." KEEP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --record) MODE=record ;;
    --shell) SHELL_UNDER_TEST=$2; shift ;;
    --uutils) UUTILS=$2; shift ;;
    --extra-path) EXTRA=$2; shift ;;
    --filter) FILTER=$2; shift ;;
    --keep) KEEP=1 ;;
    *) echo "unknown arg $1" >&2; exit 2 ;;
  esac
  shift
done

GITBASH=/usr/bin/bash
ROOT=$(mktemp -d "${SKUA_SH0_TMP:-${TMP:-/tmp}}/skua-sh0-XXXXXX")
GITCMD=/cmd   # Git for Windows cmd dir: git.exe only, no MSYS tools
SYS=/c/Windows/System32:/c/Windows:/c/Windows/System32/WindowsPowerShell/v1.0
TIMEOUT=/usr/bin/timeout

export GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=Test GIT_AUTHOR_EMAIL=t@example.com
export GIT_COMMITTER_NAME=Test GIT_COMMITTER_EMAIL=t@example.com GIT_PAGER=cat PAGER=cat
export GIT_AUTHOR_DATE='2026-01-01T00:00:00Z' GIT_COMMITTER_DATE='2026-01-01T00:00:00Z'
export TERM=dumb LC_ALL=C.UTF-8

pass=0 fail=0 crlf=0 timeouts=0
declare -A cat_pass cat_total
failed=()
for f in "$HERE"/corpus/*.sh; do
  name=$(basename "$f" .sh)
  [[ $name =~ $FILTER ]] || continue
  box="$ROOT/$name"; mkdir -p "$box/work" "$box/home"
  # Copies without CR: a CRLF checkout (core.autocrlf=true) must not change what runs.
  tr -d '\r' < "$f" > "$box/case.sh"; tr -d '\r' < "$HERE/fixture.sh" > "$box/fixture.sh"
  ( cd "$box/work" && HOME="$box/home" PATH="/mingw64/bin:/usr/bin:$SYS" "$GITBASH" --noprofile --norc "$box/fixture.sh" ) \
    > "$box/fixture.log" 2>&1 || { echo "FIXTURE FAIL $name"; cat "$box/fixture.log"; continue; }
  wcase=$(cygpath -w "$box/case.sh")
  if [ "$MODE" = record ] || [ "$SHELL_UNDER_TEST" = gitbash ]; then
    ( cd "$box/work" && HOME="$box/home" PATH="/mingw64/bin:/usr/bin:$SYS" \
        "$TIMEOUT" 20 "$GITBASH" --noprofile --norc "$wcase" ) > "$box/out" 2> "$box/err" < /dev/null
  else
    P="$UUTILS${EXTRA:+:$EXTRA}:$GITCMD:$SYS"
    ( cd "$box/work" && HOME="$box/home" PATH="$P" \
        "$TIMEOUT" 20 "$SHELL_UNDER_TEST" --noprofile --norc --no-config "$wcase" ) > "$box/out" 2> "$box/err" < /dev/null
  fi
  rc=$?
  echo "[exit $rc]" >> "$box/out"
  if [ "$MODE" = record ]; then
    tr -d '\r' < "$box/out" > "$HERE/expected/$name.txt"
    echo "recorded $name (rc=$rc)"
    continue
  fi
  c=${name#*-}; c=${c%%-*}
  cat_total[$c]=$(( ${cat_total[$c]:-0} + 1 ))
  if diff -q <(tr -d '\r' < "$box/out") <(tr -d '\r' < "$HERE/expected/$name.txt") > /dev/null; then
    pass=$((pass+1)); cat_pass[$c]=$(( ${cat_pass[$c]:-0} + 1 ))
    cmp -s "$box/out" <(tr -d '\r' < "$box/out") || crlf=$((crlf+1))
  else
    fail=$((fail+1)); failed+=("$name")
    [ $rc -eq 124 ] && timeouts=$((timeouts+1))
    echo "FAIL $name (rc=$rc)"
    diff <(tr -d '\r' < "$HERE/expected/$name.txt") <(tr -d '\r' < "$box/out") | head -8 | sed 's/^/    /'
    head -c 400 "$box/err" | sed 's/^/    err: /'
  fi
done
if [ "$MODE" = compare ]; then
  total=$((pass+fail))
  echo
  echo "shell: $SHELL_UNDER_TEST"
  for c in $(printf '%s\n' "${!cat_total[@]}" | sort); do
    echo "  $c: ${cat_pass[$c]:-0}/${cat_total[$c]}"
  done
  echo "PASS $pass/$total ($(( total ? pass * 1000 / total : 0 )) per mille); timeouts $timeouts; passes with CRLF output $crlf"
fi
[ $KEEP -eq 1 ] && echo "kept $ROOT" || rm -rf "$ROOT"
