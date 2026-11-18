#!/bin/sh
# Régénère les logos du paquet MSIX (Microsoft Store) depuis packaging/msix/icon.svg.
# ImageMagick 7 requis (commande « magick »). Les PNG produits sont versionnés : ce script ne
# sert qu'après une modification de l'icône.
set -eu
cd "$(dirname "$0")/.."
svg=packaging/msix/icon.svg
out=packaging/msix/Assets
bg='#242b4a'
render() { magick -background none -density 384 "$svg" -resize "$1x$1" "$2"; }
# Logo carré centré sur un fond de la couleur de l'icône, pour la tuile large.
banner() { magick -size "$1x$2" "xc:$bg" \( -background none -density 384 "$svg" -resize "$3x$3" \) -gravity center -composite "$4"; }

rm -rf "$out"
mkdir -p "$out"
for scale in 100 200; do
  render $((44 * scale / 100)) "$out/Square44x44Logo.scale-$scale.png"
  render $((150 * scale / 100)) "$out/Square150x150Logo.scale-$scale.png"
  render $((50 * scale / 100)) "$out/StoreLogo.scale-$scale.png"
  banner $((310 * scale / 100)) $((150 * scale / 100)) $((120 * scale / 100)) "$out/Wide310x150Logo.scale-$scale.png"
done
for size in 16 24 32 48 256; do
  render "$size" "$out/Square44x44Logo.targetsize-$size.png"
  render "$size" "$out/Square44x44Logo.targetsize-${size}_altform-unplated.png"
done
