# Vocalise

Narration audio avec **votre propre voix**, en local, sans service en ligne : vous enregistrez
quelques échantillons, puis Vocalise fait lire n'importe quel texte avec ce timbre. Pensé pour
sonoriser des vidéos et des animations (cours, capsules, tutoriels).

Vocalise ne synthétise rien lui-même : il pilote **AllTalk v2**, un serveur de synthèse vocale
local, avec le moteur **XTTS v2** (clonage de voix à partir de 6 à 30 s d'audio). Vocalise ajoute
ce qu'AllTalk n'a pas : une interface simple, la gestion des jeux de voix, l'enregistrement guidé
des échantillons, l'export FLAC/MP3/OGG, et un script en ligne de commande pour l'automatisation.

Deux outils :

| Outil | Rôle |
|---|---|
| `vocalise-gui` | Application de bureau (Rust, egui) : synthèse, enregistrement, jeux de voix, réglages, diagnostic. |
| `vocalise` (`vocalise.py`) | Script en ligne de commande : texte → wav, pour les scripts et les assistants. |

## Prérequis

- **Linux** avec **PipeWire** (Ubuntu 22.04 et suivants, Fedora, etc. ; développé sur Ubuntu 26.10 / GNOME 51).
- **Carte graphique NVIDIA** avec au moins 4 Go de mémoire, pilote installé (XTTS fonctionne aussi
  sur processeur seul, mais plusieurs fois plus lentement que le temps réel).
- Environ **8 Go de disque** pour AllTalk, son environnement Python et le modèle XTTS.
- Un **micro correct** (USB ou sans fil) et une pièce calme pour les échantillons.

## Installation

Vocalise vérifie lui-même ces composants au démarrage (Réglages → Diagnostic) et affiche, pour
chaque élément manquant, le site officiel et la commande d'installation.

### 1. AllTalk v2 et le moteur XTTS

Site officiel : <https://github.com/erew123/alltalk_tts> (branche `alltalkbeta` = version 2 ;
documentation dans le wiki du dépôt).

```bash
git clone -b alltalkbeta https://github.com/erew123/alltalk_tts ~/alltalk_tts
cd ~/alltalk_tts
./atsetup.sh
```

Dans le menu d'`atsetup.sh`, choisir l'installation **Standalone** : le script installe un
Miniconda privé dans `alltalk_tts/alltalk_environment/` (il n'a pas besoin de Python système et ne
touche pas à vos autres environnements). Comptez 10 à 20 minutes et plusieurs Go de téléchargement.

Puis lancer le serveur une première fois :

```bash
cd ~/alltalk_tts && ./start_alltalk.sh
```

Au premier démarrage, AllTalk propose de télécharger un modèle. Il faut le moteur **XTTS**
(modèle `xttsv2_2.0.3`, environ 2 Go) : il se télécharge depuis l'interface web
<http://127.0.0.1:7852>, onglet *TTS Engine Settings* → *XTTS*. Le dossier
`~/alltalk_tts/models/xtts/xttsv2_2.0.3/` doit ensuite contenir `model.pth`.

Si AllTalk démarre sur un autre moteur (Piper par défaut), Vocalise le détecte et propose un bouton
**Configurer XTTS** dans le diagnostic (il faut redémarrer AllTalk ensuite). À la main, c'est la clé
`engine_loaded` du fichier `~/alltalk_tts/system/tts_engines/tts_engines.json`. N'utilisez pas le
bouton de changement de moteur de l'interface web d'AllTalk : il plante dans cette version.

Vérification : `curl -s 127.0.0.1:7851/api/ready` doit répondre `Ready`.

### 2. Outils système

```bash
sudo apt install pipewire-bin pulseaudio-utils ffmpeg
```

- **PipeWire** (`pw-record`, `pw-play`, `pactl`) : micro et écoute. Site : <https://pipewire.org/>.
  Déjà présent sur la plupart des distributions récentes.
- **ffmpeg** : formats FLAC, MP3 et OGG Opus. Sans lui, seul le WAV est proposé.
  Site : <https://ffmpeg.org/download.html>.
- **Pilote NVIDIA** : `sudo ubuntu-drivers install` sur Ubuntu, sinon <https://www.nvidia.com/drivers>.

### 3. Vocalise (application de bureau)

**Méthode recommandée : le paquet `.deb`** (Ubuntu, Debian et dérivés). Téléchargez le fichier
`vocalise_<version>_amd64.deb` de la dernière publication sur
<https://github.com/Belenos-Toutatis/Vocalise/releases/latest>, puis :

```bash
sudo apt install ./vocalise_<version>_amd64.deb
```

Le paquet installe la commande `vocalise-gui`, les icônes et le lanceur **Vocalise** dans le menu
des applications, et tire ses dépendances (PipeWire, `pactl`, bibliothèques graphiques) ; `ffmpeg`
est recommandé pour les formats FLAC/MP3/OGG. Désinstallation : `sudo apt remove vocalise`.

**Mises à jour** : au démarrage (une fois par jour au plus), Vocalise demande à GitHub quelle est
la dernière version publiée. Si elle est plus récente, un avis apparaît en haut de la fenêtre et
Réglages → **Mises à jour** propose le téléchargement du nouveau `.deb`, à installer de la même
façon. Rien n'est téléchargé ni installé sans vous ; la vérification automatique se désactive dans
la même carte. C'est le seul accès réseau de Vocalise en dehors de la machine.

Pour **fabriquer le paquet** vous-même (chaîne Rust installée, voir ci-dessous) :

```bash
./installer.sh --deb
```

Il installe `cargo-deb` si besoin, compile en mode release et écrit
`gui/target/debian/vocalise_<version>_amd64.deb`. La configuration du paquet est la section
`[package.metadata.deb]` de `gui/Cargo.toml`.

**Depuis les sources** (sans paquet, installation pour l'utilisateur courant seulement). Il faut
la chaîne Rust (<https://rustup.rs>) et quelques bibliothèques de développement :

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
sudo apt install build-essential pkg-config libgtk-3-dev librsvg2-bin
```

Puis, depuis ce dossier :

```bash
./installer.sh
```

Le script compile en mode release (une à deux minutes la première fois), installe le binaire
`~/.local/bin/vocalise-gui`, les icônes et le lanceur **Vocalise** dans le menu des applications.
Relancez-le après toute mise à jour du code.

### 4. Le script en ligne de commande (facultatif)

```bash
ln -sf "$PWD/vocalise.py" ~/.local/bin/vocalise
```

Il n'a besoin que de Python 3 (bibliothèque standard) et de `sox` pour recoller les textes longs
(`sudo apt install sox`).

### 5. Votre première voix

1. Lancer AllTalk (ou laisser Vocalise le faire depuis Réglages).
2. Ouvrir Vocalise → **Ma voix** → *Nouveau jeu…* (par exemple votre prénom, sans accent).
3. Choisir un texte modèle, le micro, cliquer sur le bouton rouge, lire le texte d'une voix
   naturelle pendant 20 à 30 s, cliquer sur Arrêter. L'échantillon est nettoyé et ajouté au jeu.
4. Recommencer avec deux à quatre autres textes : AllTalk combine jusqu'à **5 échantillons de 30 s**
   pour calculer l'empreinte vocale. Au-delà de 5, il en tire 5 au hasard.
5. **Synthèse** → voix « Jeu de voix : votre-jeu » → un texte → Générer.

Le script `vocalise` utilise par défaut le jeu `emmanuel` (modifiable avec `--voix voiceset:nom`).

## Utilisation

### Application

- **Synthèse** : texte (saisie ou *Ouvrir un texte…* .txt/.md), voix, vitesse, température, nom et
  format du fichier (WAV, FLAC sans perte, MP3, OGG Opus), bouton Générer avec progression, écoute,
  historique de la session. Fichiers dans `sorties/`.
- **Ma voix** : jeux de voix (créer, choisir, réinitialiser, supprimer, ouvrir le dossier), jauge
  « N / 5 échantillons exploités », liste des échantillons (écouter, retirer, exporter en voix simple),
  enregistrement guidé avec chrono et vumètre.
- **Réglages** : diagnostic des composants, serveur AllTalk (démarrer, arrêter, interface web,
  journal), dossiers, langue, format par défaut, mises à jour. Configuration dans
  `~/.config/vocalise/config.json`.

Options : `--capture <dossier>` (captures PNG de chaque écran puis fermeture), `--theme clair|sombre`,
`--taille LxH`. Si la carte graphique refuse d'ouvrir la fenêtre (mémoire vidéo occupée par AllTalk),
l'application se relance d'elle-même en rendu logiciel.

### Ligne de commande

```bash
vocalise -o narration.wav "Le texte à dire."
vocalise -f texte.txt -o narration.wav          # texte long, découpé par phrases
vocalise --voix voiceset:emmanuel --vitesse 1.1 --temperature 0.5 -o s.wav "…"
```

Conseils de rédaction : nombres en toutes lettres (« vingt-cinq degrés »), ponctuation nette,
pas d'abréviations.

## Vos fichiers

Rien n'est jamais supprimé par Vocalise.

| Emplacement | Contenu |
|---|---|
| `~/alltalk_tts/voices/xtts_multi_voice_sets/<jeu>/` | Les échantillons nettoyés (30 s max) de chaque jeu de voix, lus par AllTalk. |
| `~/alltalk_tts/voices/*.wav` | Voix simples (un seul fichier), dont celles exportées depuis un jeu. |
| `echantillons/` | Enregistrements bruts, datés (utiles plus tard pour un fine-tuning). |
| `echantillons/corbeille/` | Échantillons retirés d'un jeu ou d'un jeu réinitialisé. |
| `sorties/` | Fichiers audio générés par l'application. |
| `tests/` | Sorties de test. |
| `gui/` | Code Rust de l'application (`cargo test` pour les tests). `gui/textes/` : textes modèles, ajoutez-y vos `.md`. |
| `CLAUDE.md` | Repères pour Claude Code (commandes, architecture, règles sur les fichiers). |

## Dépannage

- **« AllTalk n'est pas démarré »** : lancez-le depuis Réglages ou avec `cd ~/alltalk_tts && ./start_alltalk.sh`.
  Le chargement du modèle prend environ une minute.
- **La fenêtre ne s'ouvre pas** (`Glutin OutOfMemory` dans le terminal) : la mémoire vidéo est pleine.
  Vocalise se relance seul en rendu logiciel ; sinon fermez d'autres applications utilisant le GPU.
- **Aucun son enregistré** : vérifiez le micro dans Ma voix (bouton ↻ pour actualiser la liste) et
  qu'il n'est pas coupé dans les réglages son du système.
- **Voix peu fidèle** : réenregistrez des échantillons plus propres (pièce calme, 15–20 cm du micro,
  voix posée), variez les textes, visez 3 à 5 échantillons. Le clonage direct a ses limites ; l'étape
  suivante est le fine-tuning XTTS proposé par AllTalk (10 à 20 min de voix, GPU conseillé).
- **FLAC/MP3/OGG absents du menu** : installez `ffmpeg`.

## Désinstallation

Installation par le paquet : `sudo apt remove vocalise`. Installation depuis les sources :

```bash
rm ~/.local/bin/vocalise-gui ~/.local/bin/vocalise
rm ~/.local/share/applications/vocalise.desktop
rm ~/.local/share/icons/hicolor/*/apps/vocalise.*
rm -r ~/.config/vocalise
```

Vos échantillons et jeux de voix restent dans `~/alltalk_tts/voices/` et dans ce dossier.
