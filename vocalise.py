#!/usr/bin/env python3
"""Vocalise : texte -> wav avec la voix clonée de l'utilisateur, via l'API locale d'AllTalk (XTTS).

Usage :
  vocalise "Bonjour à tous" -o sortie.wav
  vocalise -f narration.txt -o sortie.wav
  vocalise --voix autre.wav --vitesse 1.1 ...
  vocalise --garder -f partie1.txt -o p1.wav   # plusieurs fichiers d'affilée…
  vocalise --arreter                           # …puis libérer la mémoire

Les textes longs sont découpés par phrases (limite API 2000 caractères),
générés morceau par morceau puis concaténés avec sox.

Gestion du serveur : si AllTalk ne tourne pas, vocalise le démarre (~1 min) et l'arrête à la fin
pour libérer la RAM et la VRAM, sauf avec --garder. Un AllTalk lancé par quelqu'un d'autre
(interface graphique, terminal) n'est jamais arrêté. Plusieurs vocalise simultanés partagent le
serveur : seul le dernier à finir l'arrête.
"""
import argparse, fcntl, json, os, re, shutil, signal, subprocess, sys, tempfile, time, urllib.request, urllib.parse, urllib.error

API = "http://127.0.0.1:7851"
# Longueur maximale d'un morceau (limite d'AllTalk : 2000). La VRAM nécessaire croît avec la
# longueur : sur la GTX 1650 (4 Go), au-delà de ~900 caractères la génération
# échoue toujours, et dès ~500 quand le bureau occupe plus de VRAM. Un morceau qui échoue quand
# même est recoupé en deux (generer_adaptatif).
MAX = 400
ALLTALK = os.path.expanduser("~/alltalk_tts")
ETAT = os.path.join(os.environ.get("XDG_CACHE_HOME") or os.path.expanduser("~/.cache"), "vocalise")
PIDFILE = os.path.join(ETAT, "alltalk.pid")    # présent = AllTalk lancé par vocalise
VERROU = os.path.join(ETAT, "utilisation.lock")  # verrou partagé tant qu'un vocalise s'en sert
JOURNAL = os.path.join(ETAT, "alltalk.log")

def info(msg):
    print(msg, file=sys.stderr, flush=True)

def est_pret():
    try:
        return urllib.request.urlopen(API + "/api/ready", timeout=3).read().decode() == "Ready"
    except (urllib.error.URLError, ConnectionError, OSError):
        return False

def groupe_vivant(pgid):
    try:
        os.killpg(pgid, 0)
        return True
    except (ProcessLookupError, PermissionError):
        return False

def pid_vocalise():
    """Groupe de processus d'un AllTalk lancé par vocalise et encore vivant, sinon None."""
    try:
        pid = int(open(PIDFILE).read().strip())
    except (OSError, ValueError):
        return None
    if groupe_vivant(pid):
        return pid
    try:
        os.remove(PIDFILE)
    except OSError:
        pass
    return None

def alltalk_en_cours():
    """Un AllTalk démarre-t-il ou tourne-t-il déjà (lancé par n'importe qui) ?"""
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            if os.readlink(f"/proc/{d}/cwd") != ALLTALK:
                continue
            cmd = open(f"/proc/{d}/cmdline", "rb").read().split(b"\0")
        except OSError:
            continue
        if any(c.endswith(b"script.py") for c in cmd) and any(b"python" in c for c in cmd):
            return True
    return False

def demarrer():
    os.makedirs(ETAT, exist_ok=True)
    script = os.path.join(ALLTALK, "start_alltalk.sh")
    if not os.path.isfile(script):
        sys.exit(f"AllTalk est introuvable : {script} n'existe pas.")
    info("AllTalk ne tourne pas : démarrage (environ une minute)…")
    with open(JOURNAL, "ab") as log:
        # Sortie non tamponnée : les erreurs du moteur arrivent tout de suite dans le journal.
        # (Pas de PYTORCH_CUDA_ALLOC_CONF=expandable_segments : essayé, il fait échouer presque
        # toutes les générations sur la GTX 1650.)
        env = dict(os.environ, PYTHONUNBUFFERED="1")
        p = subprocess.Popen(["bash", "-c", f"cd '{ALLTALK}' && exec ./start_alltalk.sh"], env=env,
                             stdin=subprocess.DEVNULL, stdout=log, stderr=log, start_new_session=True)
    tmp = PIDFILE + ".tmp"
    with open(tmp, "w") as f:
        f.write(str(p.pid))
    os.rename(tmp, PIDFILE)
    return p

def arreter(pgid):
    """Ctrl-C au groupe d'AllTalk, puis SIGTERM, puis SIGKILL : la VRAM est rendue à la fin."""
    info("Arrêt d'AllTalk (libération de la mémoire)…")
    for sig, delai in ((signal.SIGINT, 10), (signal.SIGTERM, 5), (signal.SIGKILL, 5)):
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            break
        fin = time.time() + delai
        while time.time() < fin and groupe_vivant(pgid):
            time.sleep(0.2)
        if not groupe_vivant(pgid):
            break
    try:
        os.remove(PIDFILE)
    except OSError:
        pass

def attendre_pret(proc=None, timeout=300):
    debut = time.time()
    while time.time() - debut < timeout:
        if est_pret():
            return
        if proc is not None and proc.poll() is not None:
            sys.exit(f"AllTalk s'est arrêté pendant son démarrage : voir {JOURNAL}")
        time.sleep(2)
    sys.exit(f"AllTalk ne répond toujours pas sur {API} après {timeout} s" + (f" : voir {JOURNAL}" if proc else ""))

def preparer_serveur():
    """Rend AllTalk prêt ; renvoie le verrou d'utilisation (partagé) à garder ouvert."""
    os.makedirs(ETAT, exist_ok=True)
    verrou = open(VERROU, "a")
    fcntl.flock(verrou, fcntl.LOCK_SH)
    if est_pret():
        return verrou
    # Le démarrage est sérialisé : deux vocalise simultanés ne lancent pas deux serveurs.
    with open(VERROU + ".demarrage", "a") as d:
        fcntl.flock(d, fcntl.LOCK_EX)
        if est_pret():
            return verrou
        if alltalk_en_cours():
            info("AllTalk est en cours de démarrage : attente…")
            attendre_pret()
        else:
            attendre_pret(demarrer())
    return verrou

def liberer_serveur(verrou, garder):
    """Arrête AllTalk s'il a été lancé par vocalise, sauf --garder ou autre vocalise en cours."""
    pgid = pid_vocalise()
    if garder or pgid is None:
        verrou.close()
        return
    try:
        fcntl.flock(verrou, fcntl.LOCK_EX | fcntl.LOCK_NB)  # échoue si un autre vocalise travaille
    except BlockingIOError:
        verrou.close()
        return
    arreter(pgid)
    verrou.close()

def commande_arreter():
    pgid = pid_vocalise()
    if pgid is not None:
        os.makedirs(ETAT, exist_ok=True)
        with open(VERROU, "a") as v:
            try:
                fcntl.flock(v, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                sys.exit("Un autre vocalise utilise AllTalk : il l'arrêtera en finissant.")
            arreter(pgid)
        print("AllTalk arrêté.")
    elif est_pret() or alltalk_en_cours():
        print("AllTalk tourne mais n'a pas été lancé par vocalise (interface graphique ou terminal) : laissé tel quel.")
    else:
        print("AllTalk ne tourne pas.")

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

def generer_une_fois(champs):
    req = urllib.request.Request(API + "/api/tts-generate",
                                 data=urllib.parse.urlencode(champs).encode())
    try:
        return json.loads(urllib.request.urlopen(req, timeout=1800).read())
    except urllib.error.HTTPError as e:
        try:
            return json.loads(e.read())
        except ValueError:
            return {"status": "generate-failure", "error": f"HTTP {e.code}"}

class EchecGeneration(Exception):
    pass

def generer(texte, voix, langue, vitesse, temperature, nom, essais=2):
    champs = {
        "text_input": texte, "text_filtering": "standard",
        "character_voice_gen": voix, "narrator_enabled": "false",
        "text_not_inside": "character", "language": langue,
        "output_file_name": nom, "output_file_timestamp": "false",
        "autoplay": "false", "speed": str(vitesse), "temperature": str(temperature),
    }
    for essai in range(1, essais + 1):
        rep = generer_une_fois(champs)
        if rep.get("status") != "generate-failure":
            break
        if essai < essais:
            time.sleep(2)
    else:
        raise EchecGeneration(json.dumps(rep, ensure_ascii=False))
    chemin = rep.get("output_file_path")
    if not chemin or not os.path.exists(chemin):
        # repli : récupérer le fichier par HTTP
        url = rep.get("output_file_url", "")
        if not url.startswith("http"):
            url = API + url
        chemin = tempfile.mktemp(suffix=".wav")
        urllib.request.urlretrieve(url, chemin)
    return chemin

MIN_COUPE = 40  # en dessous, on ne coupe plus : l'échec est définitif

def couper_en_deux(texte):
    """Coupe près du milieu : fin de phrase, sinon ponctuation faible, sinon espace."""
    milieu = len(texte) // 2
    for motif in (r"[.!?…]\s", r"[,;:]\s", r"\s"):
        positions = [m.end() for m in re.finditer(motif, texte) if 0 < m.end() < len(texte)]
        if positions:
            p = min(positions, key=lambda x: abs(x - milieu))
            return texte[:p].strip(), texte[p:].strip()
    return texte[:milieu], texte[milieu:]

def generer_adaptatif(texte, generer_morceau):
    """Avec une carte de 4 Go, la VRAM manque parfois (« CUDA out of memory »,
    « CUFFT_INTERNAL_ERROR ») et la mémoire nécessaire croît avec la longueur du texte : en cas
    d'échec répété, on coupe le morceau en deux et on recommence. Renvoie la liste des wav."""
    try:
        return [generer_morceau(texte)]
    except EchecGeneration as e:
        if len(texte) < MIN_COUPE:
            sys.exit("Échec AllTalk même sur un morceau très court : " + str(e)
                     + ". Cause probable : VRAM insuffisante ; fermer des applications gourmandes"
                     + " (navigateur, visionneuses) puis réessayer.")
        a, b = couper_en_deux(texte)
        info(f"    échec d'AllTalk (VRAM presque pleine) : morceau recoupé en {len(a)} + {len(b)} caractères")
        return generer_adaptatif(a, generer_morceau) + generer_adaptatif(b, generer_morceau)

def main():
    ap = argparse.ArgumentParser(description="Vocalise : synthèse vocale avec ma voix (AllTalk/XTTS)")
    ap.add_argument("texte", nargs="?", help="texte à dire")
    ap.add_argument("-f", "--fichier", help="fichier texte à lire")
    ap.add_argument("-o", "--sortie", help="wav de sortie")
    ap.add_argument("--voix", default=None,
                    help="fichier dans ~/alltalk_tts/voices ou 'voiceset:nom' (défaut : voiceset:emmanuel s'il existe, sinon emmanuel.wav)")
    ap.add_argument("--langue", default="fr")
    ap.add_argument("--vitesse", type=float, default=1.0)
    ap.add_argument("--temperature", type=float, default=0.7)
    ap.add_argument("--garder", action="store_true",
                    help="laisser AllTalk chargé après la génération (enchaîner plusieurs fichiers)")
    ap.add_argument("--arreter", action="store_true",
                    help="arrêter l'AllTalk lancé par vocalise, puis quitter")
    a = ap.parse_args()
    if a.arreter:
        commande_arreter()
        return
    if not a.sortie:
        ap.error("l'option -o/--sortie est obligatoire")
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
    # Un SIGTERM (fin de session, délai dépassé) passe par les blocs finally comme un Ctrl-C.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    try:
        verrou = preparer_serveur()
    except BaseException:
        # Interrompu pendant le démarrage : ne pas laisser un AllTalk à moitié chargé.
        pgid = pid_vocalise()
        if pgid is not None:
            arreter(pgid)
        raise
    try:
        base = "vocalise_" + str(int(time.time()))
        fichiers = []
        compteur = iter(range(10**6))
        def generer_morceau(m):
            return generer(m, a.voix, a.langue, a.vitesse, a.temperature, f"{base}_{next(compteur):03d}")
        morceaux = decouper(texte)
        for i, m in enumerate(morceaux):
            info(f"[{i+1}/{len(morceaux)}] {len(m)} caractères...")
            fichiers += generer_adaptatif(m, generer_morceau)
    finally:
        liberer_serveur(verrou, a.garder)
    sortie = os.path.abspath(a.sortie)
    os.makedirs(os.path.dirname(sortie), exist_ok=True)
    if len(fichiers) == 1:
        shutil.copy(fichiers[0], sortie)
    else:
        subprocess.run(["sox", "-V1", *fichiers, sortie], check=True)
    print(sortie)

if __name__ == "__main__":
    main()
