#!/usr/bin/env bash
# Time karka against Remotion on the same scene (bench/scene.html, 300 frames, 1080x1920).
#
#   bench/run.sh "karka:--software" "karka:--software --mode multi --workers 4" "remotion:--hardware-acceleration=if-possible"
#
# Each argument is <renderer>:<extra flags>; every config runs RUNS times (default 3).
# Remotion runs in REMOTION_DIR (default ~/Test/my-video) on its src/karka-bench entry point.
# karka must already be built into target/bundle (macOS) with release binaries; see bench/README.
set -uo pipefail
cd "$(dirname "$0")/.."

RUNS=${RUNS:-3}
REMOTION_DIR=${REMOTION_DIR:-$HOME/Test/my-video}
# Port 3000 may be taken by another dev server; Remotion's render server needs a free one.
REMOTION_PORT=${REMOTION_PORT:-3917}
if [[ $(uname -s) == Darwin ]]; then KARKA=target/bundle/karka.app/Contents/MacOS/karka; else KARKA=target/release/karka; fi

now() { python3 -c 'import time; print(time.time())'; }

for config in "$@"; do
  renderer=${config%%:*}
  flags=${config#*:}
  [[ $config == *:* ]] || flags=""
  for run in $(seq 1 "$RUNS"); do
    start=$(now)
    case $renderer in
      karka)
        # shellcheck disable=SC2086
        output=$("$KARKA" smoke --page bench/scene.html --out out/bench --json $flags 2>/dev/null)
        # No JSON at all means karka crashed.
        result=$(python3 -c 'import json,sys; d=json.loads(sys.argv[1]); print("ok" if d["ok"] else "FAIL: " + d["error"])' "$output" 2>/dev/null || echo "FAIL: crashed (exit code above 128 is a signal)") ;;
      remotion)
        # shellcheck disable=SC2086
        result=$(cd "$REMOTION_DIR" && npx remotion render src/karka-bench/index.ts KarkaBench out/karka-bench.mp4 \
          --port="$REMOTION_PORT" $flags >/dev/null 2>&1 && echo ok || echo FAIL) ;;
      *) echo "unknown renderer: $renderer" >&2; exit 1 ;;
    esac
    end=$(now)
    printf '%-9s %-45s run %d  %-6s %6.2fs\n' "$renderer" "$flags" "$run" "${result%%:*}" "$(python3 -c "print($end - $start)")"
    [[ $result == ok ]] || echo "          $result"
  done
done
