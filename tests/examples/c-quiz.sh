#!/usr/bin/env bash
# End to end: build examples/c-quiz, render it with karka (one browser, then two worker processes),
# and check the video has every frame and every sound starts on time.
#
#   tests/examples/c-quiz.sh        exit code 0 when everything passes
#
# With 450 frames the quiz's timer starts at frame 84 and the answer at frame 360 (see Quiz.tsx),
# so ticks start at 2.8, 3.8 ... 11.8 s and the 1.2 s chime at 12.0 s. The chime fades out, so
# silencedetect also reports the track's end, 13.2 s.
set -euo pipefail
cd "$(dirname "$0")/../.."

if [[ -z ${KARKA:-} ]]; then
  if [[ $(uname -s) == Darwin ]]; then KARKA=target/bundle/karka.app/Contents/MacOS/karka; else KARKA=target/release/karka; fi
fi
out=out/examples/c-quiz
mkdir -p "$out"
expected="2.8 3.8 4.8 5.8 6.8 7.8 8.8 9.8 10.8 11.8 12.0 13.2"

pnpm --filter c-quiz build >/dev/null

failed=0
for mode in "--mode single" "--mode multi --workers 2"; do
  mp4=$out/quiz-${mode// /_}.mp4
  # shellcheck disable=SC2086
  json=$("$KARKA" render examples/c-quiz/dist/index.html --frames 450 --software $mode \
    --props "$(cat examples/c-quiz/props.json)" --out "$mp4" --json)
  frames=$(ffprobe -v error -count_frames -select_streams v -show_entries stream=nb_read_frames -of csv=p=0 "$mp4")
  ffmpeg -loglevel error -y -i "$mp4" -vn "$mp4.wav"
  # Each place where sound starts after silence (the test tone and chime start at full level).
  starts=$(ffmpeg -hide_banner -i "$mp4.wav" -af silencedetect=noise=-40dB:d=0.05 -f null - 2>&1 |
    grep -o 'silence_end: [0-9.]*' | cut -d' ' -f2 | tr '\n' ' ')
  problems=()
  [[ $json == *'"ok":true'* && $json == *'"audio_clips":11'* ]] || problems+=("render: $json")
  [[ $frames == 450 ]] || problems+=("$frames video frames, expected 450")
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
  if [[ ${#problems[@]} == 0 ]]; then
    echo "ok      $mode"
  else
    echo "FAILED  $mode"; printf '        %s\n' "${problems[@]}"
    failed=$((failed + 1))
  fi
done
echo "$failed failed"
exit "$failed"
