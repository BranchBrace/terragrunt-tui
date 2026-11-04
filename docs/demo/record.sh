#!/usr/bin/env bash
# Régénère les GIFs du README depuis la racine du dépôt (Windows, PowerShell disponible).
# Prérequis : vhs, ttyd, ffmpeg, terragrunt et terraform sur le PATH.
set -euo pipefail

cargo build --release --locked
for tape in review run; do
  vhs "docs/demo/$tape.tape"
  # Réencodage : 12 i/s et palette réduite, pour garder des GIFs légers sur GitHub.
  ffmpeg -v error -y -i "docs/demo/$tape.gif" \
    -vf "fps=12,split[a][b];[a]palettegen=max_colors=64:stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle" \
    "docs/demo/$tape.min.gif"
  mv "docs/demo/$tape.min.gif" "docs/demo/$tape.gif"
done
