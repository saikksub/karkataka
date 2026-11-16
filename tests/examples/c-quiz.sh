#!/usr/bin/env bash
# End to end: render the quiz project (examples/c-quiz/quiz.json, one component clip) with the
# SDK, and check the video has every frame and every sound starts on time.
#
#   tests/examples/c-quiz.sh        exit code 0 when everything passes
#
# The quiz clip is 450 frames, so its timer starts at frame 84 and the answer at frame 360 (see
# Quiz.tsx): ticks start at 2.8, 3.8 ... 11.8 s and the 1.2 s chime at 12.0 s. The chime fades
# out, so silencedetect also reports the track's end, 13.2 s.
set -euo pipefail
cd "$(dirname "$0")/../.."

if [[ -z ${KARKA:-} ]]; then
  if [[ $(uname -s) == Darwin ]]; then KARKA=$PWD/target/bundle/karka.app/Contents/MacOS/karka; else KARKA=$PWD/target/release/karka; fi
fi
export KARKA
out=out/examples/c-quiz
mkdir -p "$out"
expected="2.8 3.8 4.8 5.8 6.8 7.8 8.8 9.8 10.8 11.8 12.0 13.2"

failed=0
for mode in "" "--software"; do
  mp4=$out/quiz${mode// /_}.mp4
  # shellcheck disable=SC2086
  json=$(node_modules/.bin/karkataka render examples/c-quiz/quiz.json --out "$mp4" $mode 2>/dev/null || true)
  frames=$(ffprobe -v error -count_frames -select_streams v -show_entries stream=nb_read_frames,width,height -of csv=p=0 "$mp4")
  ffmpeg -loglevel error -y -i "$mp4" -vn "$mp4.wav"
  # Each place where sound starts after silence (the test tone and chime start at full level).
  starts=$(ffmpeg -hide_banner -i "$mp4.wav" -af silencedetect=noise=-40dB:d=0.05 -f null - 2>&1 |
    grep -o 'silence_end: [0-9.]*' | cut -d' ' -f2 | tr '\n' ' ')
  problems=()
  [[ $json == *'"ok": true'* && $json == *'"audio_clips": 11'* ]] || problems+=("render: $json")
  [[ $frames == "1080,1920,450" ]] || problems+=("video is $frames, expected 1080,1920,450")
  read -ra got <<<"$starts"
  read -ra want <<<"$expected"
  if [[ ${#got[@]} != "${#want[@]}" ]]; then
    problems+=("sounds start at: $starts; expected: $expected")
  else
    for i in "${!want[@]}"; do
      awk -v g="${got[$i]}" -v w="${want[$i]}" 'BEGIN { exit !(g - w < 0.03 && w - g < 0.03) }' ||
        problems+=("sound $((i + 1)) starts at ${got[$i]} s, expected ${want[$i]} s")
    done
  fi
  label=${mode:-gpu}
  if [[ ${#problems[@]} == 0 ]]; then
    echo "ok      $label"
  else
    echo "FAILED  $label"; printf '        %s\n' "${problems[@]}"
    failed=$((failed + 1))
  fi
done
echo "$failed failed"
exit "$failed"
