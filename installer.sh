#!/usr/bin/env bash
# Compile l'interface graphique Vocalise et l'installe pour l'utilisateur courant
# (binaire dans ~/.local/bin, icône et lanceur dans ~/.local/share).
# Option --deb : fabrique un paquet Debian/Ubuntu avec cargo-deb au lieu d'installer.
set -euo pipefail
ICI="$(cd "$(dirname "$0")" && pwd)"
cd "$ICI/gui"
if [ "${1:-}" = "--deb" ]; then
    command -v cargo-deb >/dev/null || cargo install cargo-deb
    cargo deb
    PAQUET="$ICI/gui/$(ls -t target/debian/vocalise_*.deb | head -n 1)"
    echo "Paquet produit : $PAQUET"
    echo "Installation : sudo apt install \"$PAQUET\""
    exit 0
fi
cargo build --release
install -Dm755 target/release/vocalise-gui "$HOME/.local/bin/vocalise-gui"
install -Dm644 assets/vocalise.svg "$HOME/.local/share/icons/hicolor/scalable/apps/vocalise.svg"
for T in 16 24 32 48 64 128 256 512; do
    install -Dm644 "assets/icons/vocalise-$T.png" "$HOME/.local/share/icons/hicolor/${T}x${T}/apps/vocalise.png"
done
mkdir -p "$HOME/.local/share/applications"
cat > "$HOME/.local/share/applications/vocalise.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Vocalise
Comment=Synthèse vocale avec ma propre voix (AllTalk/XTTS)
Exec=$HOME/.local/bin/vocalise-gui
Icon=vocalise
Terminal=false
Categories=AudioVideo;Audio;Utility;
Keywords=voix;tts;synthèse;narration;
StartupWMClass=vocalise-gui
DESKTOP
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
# -t : pas d'index.theme dans le hicolor utilisateur ; -f : remplace un cache périmé (sinon GNOME ignore les nouvelles icônes).
gtk-update-icon-cache -q -t -f "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
echo "Vocalise installé : commande vocalise-gui, lanceur « Vocalise » dans le menu."
