#!/usr/bin/env bash
# End to end, as an AI agent would work: build a 16:9 edit from media using only `karka`
# commands, check it, look at stills, render it, and check the pictures and sound.
#
#   tests/examples/edit.sh        exit code 0 when everything passes
#
# The media are solid colours (examples/edit/make-media.sh), so a pixel says which clip is showing.
set -euo pipefail
cd "$(dirname "$0")/../.."
if [[ -z ${KARKA:-} ]]; then
  if [[ $(uname -s) == Darwin ]]; then KARKA=$PWD/target/bundle/karka.app/Contents/MacOS/karka; else KARKA=$PWD/target/release/karka; fi
fi
k() { "$KARKA" --json "$@"; }
dir=out/examples/edit
rm -rf "$dir" && mkdir -p "$dir"
p=$dir/project
k new "$p" --empty >/dev/null
examples/edit/make-media.sh "$p/media" >/dev/null

failed=0
check() { # check <what> <got> <expected>
  if [[ $2 == "$3" ]]; then echo "ok      $1"; else echo "FAILED  $1: got $2, expected $3"; failed=$((failed + 1)); fi
}
# The colour at x,y of a PNG, or of frame n of a video: red, green, blue, white, black or rgb(...).
colour() {
  local rgb
  if [[ $1 == *.png ]]; then rgb=$(ffmpeg -loglevel error -i "$1" -vf "crop=1:1:$2:$3" -f rawvideo -pix_fmt rgb24 - | od -An -tu1)
  # RGB before the 1-pixel crop: in 4:2:0 video a 1-pixel crop has no colour planes.
  else rgb=$(ffmpeg -loglevel error -i "$1" -vf "select=eq(n\,$4),format=rgb24,crop=1:1:$2:$3" -frames:v 1 -f rawvideo -pix_fmt rgb24 - | od -An -tu1); fi
  read -r r g b <<<"$rgb"
  if ((r > 200 && g < 60 && b < 60)); then echo red
  elif ((g > 100 && r < 60 && b < 60)); then echo green
  elif ((b > 200 && r < 60 && g < 60)); then echo blue
  elif ((r > 200 && g > 200 && b > 200)); then echo white
  elif ((r < 30 && g < 30 && b < 30)); then echo black
  else echo "rgb($r,$g,$b)"; fi
}
json() { node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const v=JSON.parse(s);console.log(eval("v."+process.argv[1]))})' "$1"; }

# Build the edit. Times are frames at 30 fps.
k add-track "$p" --id V1 --name Video >/dev/null
k add-track "$p" --id V2 --name Overlays >/dev/null
k add-track "$p" --id A1 --name Music >/dev/null
# The clip without its first second; no duration given: the rest of the file (4 s - 1 s = 90 frames).
check "video duration from the file" "$(k add "$p" --track V1 type=video src=media/clip.mp4 trim=30 | json result.clip.duration)" 90
# Cut it at frame 40: the second part plays on from source frame 70.
check "split keeps the source going" "$(k split "$p" video-1 --at 40 | json 'result.clips[1].trim')" 70
k add "$p" --track V2 type=image src=media/logo.png start=20 duration=40 box.x=1500 box.y=60 box.width=360 box.height=360 fit=contain >/dev/null
k add "$p" --track V2 type=text text="The title" start=60 duration=30 style.fontSize=120 style.color=white fadeIn=10 >/dev/null
k add "$p" --track A1 type=audio src=media/music.wav duration=90 volume=0.5 >/dev/null
# A mistake is refused with a reason, and the file is left alone.
check "overlap refused" "$(k add "$p" --track V2 type=color color=red start=50 duration=20 | json ok)" false
check "project is valid" "$(k validate "$p" | json ok)" true
check "length" "$(k show "$p" | json result.duration)" 90

# Look at it, as an agent checks its work.
k still "$p" --frame 10 --out "$dir/f10.png" >/dev/null
k still "$p" --frame 30 --out "$dir/f30.png" >/dev/null
k still "$p" --frame 50 --out "$dir/f50.png" >/dev/null
check "frame 10: video, source frame 40 (red)" "$(colour "$dir/f10.png" 960 540)" red
check "frame 10: no logo yet" "$(colour "$dir/f10.png" 1680 240)" red
check "frame 30: logo in its box" "$(colour "$dir/f30.png" 1680 240)" blue
check "frame 50: second part, source frame 80 (green)" "$(colour "$dir/f50.png" 960 540)" green
check "frame 50: logo still there" "$(colour "$dir/f50.png" 1680 240)" blue

# Render, and check the video: size, length, sound, and the same pictures.
result=$(k render "$p" --out "$dir/edit.mp4" --progress 2>"$dir/progress.log")
check "render ok" "$(json ok <<<"$result")" true
check "audio clips (2 video parts + music)" "$(json result.audio_clips <<<"$result")" 3
check "progress lines" "$(wc -l <"$dir/progress.log" | tr -d ' ')" 90
check "video stream" "$(ffprobe -v error -select_streams v -show_entries stream=width,height,r_frame_rate -count_frames -show_entries stream=nb_read_frames -of csv=p=0 "$dir/edit.mp4")" "1920,1080,30/1,90"
check "audio stream" "$(ffprobe -v error -select_streams a -show_entries stream=codec_name -of csv=p=0 "$dir/edit.mp4")" aac
# The source turns green at its frame 60; with trim 30 that is video frame 30, exactly.
check "video frame 29: source frame 59 (red)" "$(colour "$dir/edit.mp4" 960 540 29)" red
check "video frame 30: source frame 60 (green)" "$(colour "$dir/edit.mp4" 960 540 30)" green
# Had the split not moved the second part's trim on, this would be source frame 40 (red).
check "video frame 50: after the split, source frame 80 (green)" "$(colour "$dir/edit.mp4" 960 540 50)" green
title=$(ffmpeg -loglevel error -i "$dir/edit.mp4" -vf "select=eq(n\,85),crop=600:120:660:480,format=gray" -frames:v 1 -f rawvideo - | od -An -tu1 | tr -s ' ' '\n' | awk '$1 > 200 {n++} END {print (n > 1000 ? "text" : "none")}')
check "video frame 85: white title text over the video" "$title" text
echo "$failed failed"
exit "$failed"
