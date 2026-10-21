#!/usr/bin/env python3
"""Vocalise : texte -> wav avec la voix clonée de l'utilisateur, via l'API locale d'AllTalk (XTTS).

Usage :
  vocalise "Bonjour à tous" -o sortie.wav
  vocalise -f narration.txt -o sortie.wav
  vocalise --voix autre.wav --vitesse 1.1 ...

Les textes longs sont découpés par phrases (limite API 2000 caractères),
générés morceau par morceau puis concaténés avec sox.
"""
import argparse, json, os, re, shutil, subprocess, sys, tempfile, time, urllib.request, urllib.parse, urllib.error

API = "http://127.0.0.1:7851"
MAX = 1800  # marge sous la limite api_max_characters (2000)

def attendre_pret(timeout=600):
    debut = time.time()
    while time.time() - debut < timeout:
        try:
            if urllib.request.urlopen(API + "/api/ready", timeout=5).read().decode() == "Ready":
                return
        except (urllib.error.URLError, ConnectionError):
            pass
        time.sleep(3)
    sys.exit("AllTalk ne répond pas sur " + API + " (lance ./start_alltalk.sh)")

def decouper(texte):
    phrases = re.split(r"(?<=[.!?…])\s+", texte.strip())
    morceaux, courant = [], ""
    for p in phrases:
        if len(courant) + len(p) + 1 > MAX and courant:
            morceaux.append(courant); courant = p
        else:
            courant = (courant + " " + p).strip()
    if courant:
        morceaux.append(courant)
    return morceaux

def generer(texte, voix, langue, vitesse, temperature, nom):
    champs = {
        "text_input": texte, "text_filtering": "standard",
        "character_voice_gen": voix, "narrator_enabled": "false",
        "text_not_inside": "character", "language": langue,
        "output_file_name": nom, "output_file_timestamp": "false",
        "autoplay": "false", "speed": str(vitesse), "temperature": str(temperature),
    }
    req = urllib.request.Request(API + "/api/tts-generate",
                                 data=urllib.parse.urlencode(champs).encode())
    try:
        rep = json.loads(urllib.request.urlopen(req, timeout=1800).read())
    except urllib.error.HTTPError as e:
        sys.exit(f"Échec AllTalk (HTTP {e.code}) : {e.read().decode(errors='replace')}")
    if rep.get("status") == "generate-failure":
        sys.exit("Échec AllTalk : " + json.dumps(rep, ensure_ascii=False))
    chemin = rep.get("output_file_path")
    if not chemin or not os.path.exists(chemin):
        # repli : récupérer le fichier par HTTP
        url = rep.get("output_file_url", "")
        if not url.startswith("http"):
            url = API + url
        chemin = tempfile.mktemp(suffix=".wav")
        urllib.request.urlretrieve(url, chemin)
    return chemin

def main():
    ap = argparse.ArgumentParser(description="Vocalise : synthèse vocale avec ma voix (AllTalk/XTTS)")
    ap.add_argument("texte", nargs="?", help="texte à dire")
    ap.add_argument("-f", "--fichier", help="fichier texte à lire")
    ap.add_argument("-o", "--sortie", required=True, help="wav de sortie")
    ap.add_argument("--voix", default=None,
                    help="fichier dans ~/alltalk_tts/voices ou 'voiceset:nom' (défaut : voiceset:emmanuel s'il existe, sinon emmanuel.wav)")
    ap.add_argument("--langue", default="fr")
    ap.add_argument("--vitesse", type=float, default=1.0)
    ap.add_argument("--temperature", type=float, default=0.7)
    a = ap.parse_args()
    if a.fichier:
        texte = open(a.fichier, encoding="utf-8").read()
    elif a.texte:
        texte = a.texte
    else:
        texte = sys.stdin.read()
    texte = re.sub(r"\s+", " ", texte).strip()
    if not texte:
        sys.exit("texte vide")

    if a.voix is None:
        jeu = os.path.expanduser("~/alltalk_tts/voices/xtts_multi_voice_sets/emmanuel")
        a.voix = "voiceset:emmanuel" if os.path.isdir(jeu) and any(f.endswith(".wav") for f in os.listdir(jeu)) else "emmanuel.wav"
    attendre_pret()
    base = "vocalise_" + str(int(time.time()))
    fichiers = []
    for i, m in enumerate(decouper(texte)):
        print(f"[{i+1}] {len(m)} caractères...", file=sys.stderr)
        fichiers.append(generer(m, a.voix, a.langue, a.vitesse, a.temperature, f"{base}_{i:03d}"))
    sortie = os.path.abspath(a.sortie)
    os.makedirs(os.path.dirname(sortie), exist_ok=True)
    if len(fichiers) == 1:
        shutil.copy(fichiers[0], sortie)
    else:
        subprocess.run(["sox", *fichiers, sortie], check=True)
    print(sortie)

if __name__ == "__main__":
    main()
