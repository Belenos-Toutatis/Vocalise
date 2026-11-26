#!/usr/bin/env python3
"""Moteur Qwen3-TTS de Vocalise : processus de travail, lancé par `vocalise` ou par l'application avec
le Python de son propre environnement (paquet qwen-tts et torch, incompatibles avec ceux d'AllTalk).

Le modèle est chargé une fois, puis chaque ligne de l'entrée standard est une demande JSON
    {"texte": "...", "sortie": "/chemin/morceau.wav"}
et reçoit sur la sortie standard une ligne JSON :
    {"ok": true, "sortie": "...", "duree": 3.2}   ou   {"ok": false, "erreur": "..."}
Avant la première demande, une ligne {"pret": true, "appareil": "cpu"} annonce la fin du chargement.
Le processus se termine à la fin de l'entrée standard : la mémoire est rendue au système.

Voix : --ref-audio (quelques secondes de la voix à imiter) et, si possible, --ref-texte (ce qui y est
dit mot pour mot ; sans lui, seul le timbre est copié, la ressemblance est moindre).
Les journaux des bibliothèques partent sur la sortie d'erreur, jamais sur la sortie standard.
"""
import argparse, json, os, sys, time, wave

LANGUES = {"fr": "French", "en": "English", "de": "German", "es": "Spanish", "it": "Italian",
           "pt": "Portuguese", "ru": "Russian", "ja": "Japanese", "ko": "Korean", "zh": "Chinese"}
MODELE = "Qwen/Qwen3-TTS-12Hz-0.6B-Base"

def ecrire_wav(chemin, echantillons, sr):
    """PCM 16 bits mono, écrit en .part puis renommé."""
    import numpy as np
    a = np.clip(np.asarray(echantillons, dtype=np.float32), -1.0, 1.0)
    tmp = chemin + ".part"
    with wave.open(tmp, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(int(sr))
        w.writeframes((a * 32767.0).astype("<i2").tobytes())
    os.replace(tmp, chemin)

def choisir_appareil(demande):
    import torch
    if demande != "auto":
        return demande
    # Le modèle ne tient sur la carte qu'en float32 (en float16, la génération plante) : ~3 Go libres
    # au minimum. Sur une carte de 4 Go dont le bureau occupe une partie, c'est le processeur.
    if torch.cuda.is_available():
        try:
            libre, _ = torch.cuda.mem_get_info()
            if libre >= 3.5 * 2**30:
                return "cuda"
        except Exception:
            pass
    return "cpu"

def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--ref-audio", required=True)
    ap.add_argument("--ref-texte", default="")
    ap.add_argument("--langue", default="fr")
    ap.add_argument("--modele", default=MODELE)
    ap.add_argument("--appareil", default="auto", choices=["auto", "cpu", "cuda"])
    a = ap.parse_args()

    # Sortie standard réservée au protocole : tout le reste (bibliothèques, avertissements) va sur
    # la sortie d'erreur, y compris ce qu'écrit du code natif sur le descripteur 1.
    proto = os.fdopen(os.dup(1), "w", encoding="utf-8", buffering=1)
    os.dup2(2, 1)
    sys.stdout = sys.stderr
    def repondre(d):
        proto.write(json.dumps(d, ensure_ascii=False) + "\n")
        proto.flush()

    try:
        import torch
        from qwen_tts import Qwen3TTSModel
        appareil = choisir_appareil(a.appareil)
        modele = Qwen3TTSModel.from_pretrained(a.modele, device_map=appareil, dtype=torch.float32)
        ref_texte = a.ref_texte.strip()
        invite = modele.create_voice_clone_prompt(
            ref_audio=a.ref_audio, ref_text=ref_texte or None, x_vector_only_mode=not ref_texte)
    except Exception as e:
        repondre({"pret": False, "erreur": f"{type(e).__name__} : {e}"})
        sys.exit(1)
    langue = LANGUES.get(a.langue.lower(), a.langue)
    repondre({"pret": True, "appareil": appareil})

    for ligne in sys.stdin:
        ligne = ligne.strip()
        if not ligne:
            continue
        try:
            d = json.loads(ligne)
            debut = time.time()
            wavs, sr = modele.generate_voice_clone(text=d["texte"], language=langue, voice_clone_prompt=invite)
            ecrire_wav(d["sortie"], wavs[0], sr)
            repondre({"ok": True, "sortie": d["sortie"], "duree": round(len(wavs[0]) / sr, 2),
                      "calcul": round(time.time() - debut, 1)})
        except Exception as e:
            repondre({"ok": False, "erreur": f"{type(e).__name__} : {e}"})

if __name__ == "__main__":
    main()
