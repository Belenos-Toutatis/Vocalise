#!/usr/bin/env bash
# Enregistre l'échantillon de référence avec le micro DJI Mic Mini,
# puis le nettoie (mono, 22,05 kHz, silences coupés, niveau normalisé)
# et le dépose dans les voix d'AllTalk sous le nom emmanuel.wav.
set -euo pipefail
MIC="alsa_input.usb-DJI_Technology_Co.__Ltd._DJI_MIC_MINI_XSP12345678B-01.analog-stereo"
BRUT="$HOME/Vocalise/echantillons/brut-$(date +%F-%H%M).wav"
DEST="$HOME/alltalk_tts/voices/emmanuel.wav"

echo "Enregistrement en cours. Lis le texte, puis appuie sur Ctrl-C pour arrêter."
pw-record --target "$MIC" --rate 48000 --channels 1 --format s16 "$BRUT" || true

echo "Nettoyage..."
sox "$BRUT" -r 22050 -c 1 -b 16 "$DEST" \
    silence 1 0.2 1% reverse silence 1 0.2 1% reverse \
    pad 0.3 0.3 norm -3
echo "Échantillon écrit dans $DEST"
soxi "$DEST" | grep -E 'Duration|Sample Rate|Channels'
