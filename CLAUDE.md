# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Ce qu'est Vocalise

Narration audio avec la voix clonée de l'utilisateur, en local. Le moteur est **AllTalk v2 + XTTS v2**
(`~/alltalk_tts`, hors de ce dépôt, env conda isolé, GPU GTX 1650 4 Go). Vocalise ne synthétise rien
lui-même : il pilote l'API HTTP d'AllTalk (`127.0.0.1:7851`) et gère les échantillons de voix.
Le code et l'interface sont en français (identifiants, messages, commentaires) : garder cette convention.

Deux outils cohabitent :
- `vocalise.py` : script CLI (`vocalise -o sortie.wav "texte"`, lien dans `~/.local/bin`). C'est lui
  que les autres sessions Claude appellent pour produire une voix off (consigne dans `~/.claude/CLAUDE.md`).
- `gui/` : application de bureau Rust (eframe/egui 0.31), binaire `vocalise-gui`.

## Commandes

```bash
cd ~/alltalk_tts && ./start_alltalk.sh      # prérequis : serveur AllTalk (~1 min), API 7851, web 7852
curl -s 127.0.0.1:7851/api/ready            # doit répondre « Ready »

cd gui && cargo test                        # tests unitaires (aucune dépendance à AllTalk/PipeWire)
cd gui && cargo test -- --ignored integration   # test réel contre AllTalk en marche
cd gui && cargo test texte::                # un module ; `cargo test nom_du_test` pour un seul test
cd gui && cargo check                       # compilation rapide
./installer.sh                              # build release + install ~/.local/bin/vocalise-gui + icônes + lanceur GNOME

vocalise-gui --capture <dossier> [--theme clair|sombre] [--taille 860x560]
```

Le mode `--capture` écrit un PNG par écran (synthese, ma-voix, reglages, confirmation,
enregistrement, synthese-sans-alltalk, diagnostic, mises-a-jour) puis quitte. **C'est le seul moyen de voir l'interface depuis
une session Claude** (pas de capture d'écran du bureau) : l'utiliser après tout changement visuel.
Il ne démarre rien, n'enregistre rien et ne sauvegarde pas la config. La fenêtre de capture s'ouvre
via X11 (XWayland), au-dessus des autres : sous Wayland, une fenêtre recouverte n'est plus redessinée
et la série se figeait. Recouverte, la série dure ~40 s au lieu de 10 s ; garde-fou à 120 s.

Pas de lint configuré. Dépôt git : `Belenos-Toutatis/Vocalise` sur GitHub (branche `main`) ; les
publications (`gh release create vX.Y.Z gui/target/debian/vocalise_X.Y.Z_amd64.deb`) alimentent la
vérification de mise à jour de l'application. Pour livrer une version : bumper `version` dans
`gui/Cargo.toml`, `./installer.sh --deb`, commit, tag `vX.Y.Z`, release avec le `.deb` joint.

## Architecture de `gui/`

Deux couches strictement séparées, conçues pour être modifiées indépendamment :

- **Métier, sans egui** : `paths` (config JSON `~/.config/vocalise/config.json` + chemins dérivés),
  `audio` (wav via hound + lecteur RIFF de repli, nettoyage, troncature, concaténation, `niveau_direct`
  qui lit la fin d'un wav en cours d'écriture, conversion ffmpeg), `texte` (découpage par phrases sous
  1800 caractères, textes modèles embarqués par `include_str!` depuis `gui/textes/`), `alltalk` (API et
  processus serveur), `voicesets` (jeux de voix), `recorder` (`pw-record` + `pactl -f json`),
  `player` (`pw-play`), `diagnostic` (composants manquants → lien officiel + commande ; bascule du
  moteur vers XTTS par édition de `tts_engines.json`), `maj` (dernière release GitHub via
  `api.github.com`, comparaison avec `CARGO_PKG_VERSION` ; jamais d'installation automatique).
  Tout l'audio système passe par des sous-processus PipeWire, pas par cpal.
- **Interface** : `app.rs` (état `App`, enum `Msg`, actions, boucle `update`) et `ui/` (`theme` palette
  claire/sombre unique, `widgets` boutons et vumètre dessinés à la main, un fichier par écran,
  `dialogues` modales, `capture`).

Règle de concurrence : toute opération longue (génération, nettoyage d'échantillon, sonde du serveur,
arrêt) tourne dans un `std::thread` et répond par le canal `mpsc` avec un `Msg`, suivi de
`ctx.request_repaint()`. L'interface ne bloque jamais, sauf les dialogues rfd (synchrones, assumé).
Pas d'`egui::Spinner` (il force 60 i/s) : utiliser `widgets::sablier`.

Pas de nouvelle dépendance sans raison forte. `ureq` a TLS (rustls) uniquement pour l'API GitHub du
module `maj` ; tout le reste parle à AllTalk en HTTP local.

## Faits AllTalk/XTTS qui dictent le code

- Un **jeu de voix** = dossier `voices/xtts_multi_voice_sets/<jeu>/*.wav`, nom API `voiceset:<jeu>`.
  AllTalk n'exploite que **5 fichiers** (tirés au hasard au-delà) et **30 s par fichier**
  (`max_ref_len`). D'où la troncature à 30 s et la jauge « N / 5 ».
- Les wav de sortie d'AllTalk sont en float 32 bits, 24 kHz, avec chunks `fact`/`PEAK`.
- `/api/enginereload` plante (bug AllTalk) : pour changer de moteur, éditer
  `~/alltalk_tts/system/tts_engines/tts_engines.json` (`engine_loaded`, `selected_model`) et redémarrer.
- La GTX 1650 est presque pleine quand AllTalk est chargé : si l'ouverture de la fenêtre échoue
  (« Glutin OutOfMemory »), `app::lancer` relance le binaire en rendu logiciel (Mesa llvmpipe).
  Ne pas « corriger » ce comportement.

## Politique sur les fichiers de l'utilisateur

- Un wav d'échantillon n'est **jamais supprimé** : retrait et réinitialisation déplacent vers
  `echantillons/corbeille/`. Les enregistrements bruts restent dans `echantillons/` (réserve pour un
  futur fine-tuning).
- Les écritures dans un dossier lu par AllTalk passent par `.part` puis `rename` ; la config par
  `.tmp` puis `rename`, et une config illisible est copiée en `.bak` avant d'être remplacée.
- Un fichier de sortie existant n'est pas écrasé (suffixe `-2`, `-3`…).
- Ne pas supprimer `~/alltalk_tts/voices/emmanuel.wav` ni le jeu `emmanuel` : ce sont la voix de
  l'utilisateur.
