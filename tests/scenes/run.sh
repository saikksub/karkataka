#!/usr/bin/env bash
# Render every test scene in every mode and check every frame (unique, in order, right frame number).
#
#   tests/scenes/run.sh                 all scenes, all modes
#   tests/scenes/run.sh webgl svg       only these scenes
#   MODES="gpu software" FRAMES=60 tests/scenes/run.sh
#
# Modes: gpu, software (single browser), gpu-multi, software-multi (2 worker processes).
# A page is _head.html + <scene>.html + _tail.html; output goes to out/scenes/<scene>/<mode>/.
# Red cells in frame0.png mean the scene itself failed (no WebGL, a JS error): see stderr.log there.
# Exit code is the number of failed runs.
set -uo pipefail
cd "$(dirname "$0")/../.."

FRAMES=${FRAMES:-120}
MODES=${MODES:-gpu software gpu-multi software-multi}
if [[ $(uname -s) == Darwin ]]; then KARKA=target/bundle/karka.app/Contents/MacOS/karka; else KARKA=target/release/karka; fi
dir=tests/scenes

scenes=("$@")
if [[ ${#scenes[@]} -eq 0 ]]; then
  for f in "$dir"/[!_]*.html; do scenes+=("$(basename "$f" .html)"); done
fi

failed=0
for scene in "${scenes[@]}"; do
  [[ -f $dir/$scene.html ]] || { echo "no scene $dir/$scene.html" >&2; exit 1; }
  for mode in $MODES; do
    out=out/scenes/$scene/$mode
    mkdir -p "$out"
    cat "$dir/_head.html" "$dir/$scene.html" "$dir/_tail.html" > "$out/page.html"
    flags=(--mode single)
    [[ $mode == *multi ]] && flags=(--mode multi --workers 2)
    [[ $mode == software* ]] && flags+=(--software)
    json=$("$KARKA" smoke --page "$out/page.html" --out "$out" --frames "$FRAMES" --json "${flags[@]}" 2>"$out/stderr.log")
    result=$(python3 -c '
import json, sys
d = json.loads(sys.argv[1])
print("ok %5.1f fps" % d["result"]["render_fps"] if d["ok"] else "FAIL " + d["error"])
' "$json" 2>/dev/null || echo "FAIL crashed (no JSON; see $out/stderr.log)")
    printf '%-16s %-15s %s\n' "$scene" "$mode" "$result"
    [[ $result == ok* ]] || failed=$((failed + 1))
  done
done
echo "$failed failed"
exit "$failed"
