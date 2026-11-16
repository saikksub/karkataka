#!/usr/bin/env bash
# Make the example's media with FFmpeg's built-in sources, so nothing large is kept in git:
#   media/clip.mp4   4 s, 1280x720 @ 30: red for 2 s, then green, with a 330 Hz tone
#   media/logo.png   400x400 blue square
#   media/music.wav  10 s, 550 Hz tone, quiet
set -euo pipefail
dir=${1:-$(dirname "$0")/media}
mkdir -p "$dir"
ff() { ffmpeg -loglevel error -y "$@"; }
ff -f lavfi -i "color=c=red:size=1280x720:rate=30:duration=2" -f lavfi -i "color=c=green:size=1280x720:rate=30:duration=2" \
   -f lavfi -i "sine=frequency=330:duration=4" \
   -filter_complex "[0][1]concat=n=2:v=1:a=0[v]" -map "[v]" -map 2 -c:v libx264 -pix_fmt yuv420p -c:a aac "$dir/clip.mp4"
ff -f lavfi -i "color=c=blue:size=400x400" -frames:v 1 "$dir/logo.png"
ff -f lavfi -i "sine=frequency=550:duration=10" -af volume=0.3 "$dir/music.wav"
echo "media in $dir"
