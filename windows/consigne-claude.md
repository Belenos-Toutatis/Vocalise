# Consignes globales

<!-- À copier dans %USERPROFILE%\.claude\CLAUDE.md sur l'ordinateur Windows (ou à ajouter à ce
     fichier s'il existe déjà), pour que les sessions Claude Code utilisent votre voix. -->

## Vocalise : voix de l'utilisateur pour les narrations (vidéos, animations, audio)

Quand une vidéo, une animation ou un contenu audio a besoin d'une voix off, **utiliser la voix clonée de l'utilisateur**, jamais Kokoro ni une autre voix générique, sauf demande explicite.

- Commande (PowerShell ou invite de commandes) : `vocalise -o sortie.wav "texte"` ou `vocalise -f texte.txt -o sortie.wav` (options `--vitesse`, `--temperature`). Sortie wav 24 kHz, à insérer telle quelle dans la composition. Installée avec Vocalise dans `%LOCALAPPDATA%\Programs\Vocalise`.
- Serveur AllTalk (API 127.0.0.1:7851) : **ne pas le lancer soi-même**. `vocalise` le démarre s'il ne tourne pas et l'arrête à la fin pour libérer la mémoire. Pour plusieurs fichiers d'affilée : `vocalise --garder …` pour chacun, puis **toujours** `vocalise --arreter` à la fin. Un AllTalk lancé par l'utilisateur ou par l'application Vocalise n'est jamais arrêté par la commande. Journal : `%LOCALAPPDATA%\vocalise\alltalk.log`.
- Cet ordinateur n'a pas de carte NVIDIA : XTTS tourne sur le processeur, la génération est lente (plusieurs minutes par minute d'audio). Prévoir un délai d'outil long et des textes découpés en plusieurs fichiers.
- Le moteur doit être **xtts** (`alltalk_tts\system\tts_engines\tts_engines.json`, clé `engine_loaded`). Ne pas utiliser `/api/enginereload`, il plante : éditer le fichier puis redémarrer le serveur.
- Voix par défaut : le jeu de voix `voiceset:emmanuel` (`alltalk_tts\voices\xtts_multi_voice_sets\emmanuel\`) ; repli `emmanuel.wav`. La commande `vocalise` choisit seule.
- Rédaction du texte à lire : nombres en toutes lettres, ponctuation marquant les pauses, pas d'abréviations.
