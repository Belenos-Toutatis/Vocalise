#!/usr/bin/env python3
"""Vocalise : texte -> wav avec la voix clonée de l'utilisateur, via l'API locale d'AllTalk (XTTS).

Usage :
  vocalise "Bonjour à tous" -o sortie.wav
  vocalise -f narration.txt -o sortie.wav
  vocalise --voix autre.wav --vitesse 1.1 ...
  vocalise --garder -f partie1.txt -o p1.wav   # plusieurs fichiers d'affilée…
  vocalise --arreter                           # …puis libérer la mémoire

Les textes longs sont découpés par phrases (limite API 2000 caractères), générés morceau par
morceau puis assemblés en un seul wav.

Gestion du serveur : si AllTalk ne tourne pas, vocalise le démarre (~1 min) et l'arrête à la fin
pour libérer la RAM et la VRAM, sauf avec --garder. Un AllTalk lancé par quelqu'un d'autre
(interface graphique, terminal) n'est jamais arrêté. Plusieurs vocalise simultanés partagent le
serveur : seul le dernier à finir l'arrête.

Fonctionne sous Linux et sous Windows (bibliothèque standard de Python uniquement). Dossier
d'AllTalk : variable ALLTALK_DIR, sinon celui des réglages de vocalise-gui, sinon ~/alltalk_tts.
"""
import argparse, json, os, re, shutil, signal, struct, subprocess, sys, tempfile, time, urllib.request, urllib.parse, urllib.error

WINDOWS = os.name == "nt"
if not WINDOWS:
    import fcntl

API = "http://127.0.0.1:7851"
# Longueur maximale d'un morceau (limite d'AllTalk : 2000). La VRAM nécessaire croît avec la
# longueur : sur la GTX 1650 (4 Go), au-delà de ~900 caractères la génération
# échoue toujours, et dès ~500 quand le bureau occupe plus de VRAM. Un morceau qui échoue quand
# même est recoupé en deux (generer_adaptatif).
MAX = 400

def dossier_config():
    if WINDOWS:
        return os.environ.get("APPDATA") or os.path.expanduser(r"~\AppData\Roaming")
    return os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")

def dossier_alltalk():
    if os.environ.get("ALLTALK_DIR"):
        return os.path.abspath(os.environ["ALLTALK_DIR"])
    try:
        with open(os.path.join(dossier_config(), "vocalise", "config.json"), encoding="utf-8") as f:
            d = json.load(f).get("alltalk_dir")
        if d and os.path.isdir(d):
            return os.path.abspath(d)
    except (OSError, ValueError, AttributeError):
        pass
    return os.path.join(os.path.expanduser("~"), "alltalk_tts")

ALLTALK = dossier_alltalk()
LANCEUR = "start_alltalk.bat" if WINDOWS else "start_alltalk.sh"
if WINDOWS:
    ETAT = os.path.join(os.environ.get("LOCALAPPDATA") or os.path.expanduser(r"~\AppData\Local"), "vocalise")
else:
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

# ------------------------------------------------------------------ processus (Linux / Windows)

if WINDOWS:
    import ctypes
    CREATE_NEW_PROCESS_GROUP = 0x00000200
    CREATE_NO_WINDOW = 0x08000000

    def processus_vivant(pid):
        """Sans os.kill : sous Windows, il terminerait le processus."""
        k = ctypes.windll.kernel32
        h = k.OpenProcess(0x1000, False, int(pid))  # PROCESS_QUERY_LIMITED_INFORMATION
        if not h:
            return False
        try:
            code = ctypes.c_ulong()
            return bool(k.GetExitCodeProcess(h, ctypes.byref(code))) and code.value == 259  # STILL_ACTIVE
        finally:
            k.CloseHandle(h)
else:
    def processus_vivant(pgid):
        """Le groupe de processus existe-t-il encore ?"""
        try:
            os.killpg(pgid, 0)
            return True
        except (ProcessLookupError, PermissionError):
            return False

def pid_vocalise():
    """Processus (Linux : groupe) d'un AllTalk lancé par vocalise et encore vivant, sinon None."""
    try:
        pid = int(open(PIDFILE).read().strip())
    except (OSError, ValueError):
        return None
    if processus_vivant(pid):
        return pid
    try:
        os.remove(PIDFILE)
    except OSError:
        pass
    return None

def alltalk_en_cours():
    """Un AllTalk démarre-t-il ou tourne-t-il déjà (lancé par n'importe qui) ?"""
    if WINDOWS:
        # Un python.exe de l'environnement d'AllTalk qui exécute script.py.
        dossier = ALLTALK.replace("'", "''")
        ps = ("@(Get-CimInstance Win32_Process -Filter \"Name='python.exe'\" | Where-Object { "
              f"$_.CommandLine -like '*script.py*' -and $_.ExecutablePath -like '{dossier}*' }}).Count")
        try:
            r = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True, text=True,
                               timeout=20, creationflags=CREATE_NO_WINDOW)
            return r.stdout.strip() not in ("", "0")
        except (OSError, subprocess.SubprocessError):
            return False
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
    script = os.path.join(ALLTALK, LANCEUR)
    if not os.path.isfile(script):
        sys.exit(f"AllTalk est introuvable : {script} n'existe pas (variable ALLTALK_DIR ou réglages de vocalise-gui).")
    info("AllTalk ne tourne pas : démarrage (environ une minute)…")
    with open(JOURNAL, "ab") as log:
        # Sortie non tamponnée : les erreurs du moteur arrivent tout de suite dans le journal.
        # (Pas de PYTORCH_CUDA_ALLOC_CONF=expandable_segments : essayé, il fait échouer presque
        # toutes les générations sur la GTX 1650.)
        env = dict(os.environ, PYTHONUNBUFFERED="1")
        if WINDOWS:
            # Chemin complet : avec NoDefaultCurrentDirectoryInExePath (défini par Claude Code),
            # cmd ne cherche plus un .bat nommé seul dans le dossier courant.
            p = subprocess.Popen(["cmd", "/D", "/C", script], cwd=ALLTALK, env=env,
                                 stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                                 creationflags=CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        else:
            p = subprocess.Popen(["bash", "-c", f"cd '{ALLTALK}' && exec ./start_alltalk.sh"], env=env,
                                 stdin=subprocess.DEVNULL, stdout=log, stderr=log, start_new_session=True)
    tmp = PIDFILE + ".tmp"
    with open(tmp, "w") as f:
        f.write(str(p.pid))
    os.replace(tmp, PIDFILE)
    return p

def arreter(pid):
    """Linux : Ctrl-C au groupe d'AllTalk, puis SIGTERM, puis SIGKILL. Windows : fin de l'arbre de
    processus (taskkill /T /F). La mémoire est rendue à la fin."""
    info("Arrêt d'AllTalk (libération de la mémoire)…")
    if WINDOWS:
        subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True,
                       creationflags=CREATE_NO_WINDOW)
        fin = time.time() + 15
        while time.time() < fin and processus_vivant(pid):
            time.sleep(0.2)
    else:
        for sig, delai in ((signal.SIGINT, 10), (signal.SIGTERM, 5), (signal.SIGKILL, 5)):
            try:
                os.killpg(pid, sig)
            except ProcessLookupError:
                break
            fin = time.time() + delai
            while time.time() < fin and processus_vivant(pid):
                time.sleep(0.2)
            if not processus_vivant(pid):
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

# ------------------------------------------------------------------ verrous entre vocalise simultanés
# Linux : verrou fcntl partagé pendant l'utilisation, exclusif pour arrêter.
# Windows : un fichier par vocalise en cours dans ETAT/utilisateurs/, et verrou msvcrt du démarrage.

class Utilisation:
    def __init__(self):
        os.makedirs(ETAT, exist_ok=True)
        if WINDOWS:
            self.dossier = os.path.join(ETAT, "utilisateurs")
            os.makedirs(self.dossier, exist_ok=True)
            self.moi = os.path.join(self.dossier, str(os.getpid()))
            open(self.moi, "w").close()
        else:
            self.f = open(VERROU, "a")
            fcntl.flock(self.f, fcntl.LOCK_SH)

    def seul(self):
        """Aucun autre vocalise ne se sert d'AllTalk ? (Linux : passe alors en verrou exclusif.)"""
        if WINDOWS:
            for nom in os.listdir(self.dossier):
                chemin = os.path.join(self.dossier, nom)
                if chemin == self.moi:
                    continue
                if nom.isdigit() and processus_vivant(int(nom)):
                    return False
                try:
                    os.remove(chemin)  # reste d'un vocalise disparu
                except OSError:
                    pass
            return True
        try:
            fcntl.flock(self.f, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return True
        except BlockingIOError:
            return False

    def fermer(self):
        if WINDOWS:
            try:
                os.remove(self.moi)
            except OSError:
                pass
        else:
            self.f.close()

class VerrouDemarrage:
    """Sérialise le démarrage : deux vocalise simultanés ne lancent pas deux serveurs."""
    def __enter__(self):
        os.makedirs(ETAT, exist_ok=True)
        self.f = open(VERROU + ".demarrage", "a+")
        if WINDOWS:
            import msvcrt
            while True:
                try:
                    self.f.seek(0)
                    msvcrt.locking(self.f.fileno(), msvcrt.LK_NBLCK, 1)
                    break
                except OSError:
                    time.sleep(0.5)
        else:
            fcntl.flock(self.f, fcntl.LOCK_EX)
        return self

    def __exit__(self, *_):
        self.f.close()  # libère le verrou
        return False

def reglages_web():
    """(interface web activée ?, son port) d'après confignew.json d'AllTalk ; (True, 7852) par défaut."""
    try:
        with open(os.path.join(ALLTALK, "confignew.json"), encoding="utf-8") as f:
            c = json.load(f)
        return c.get("gradio_interface", True) is not False, int(c.get("gradio_port_number", 7852))
    except (OSError, ValueError, TypeError, AttributeError):
        return True, 7852

def attendre_interface_web(timeout=180):
    """Une fois l'API prête, AllTalk construit encore son interface web, qui interroge l'API avec un
    délai de 10 s : si une génération (plus de 10 s sur processeur) occupe l'API à ce moment-là,
    script.py plante (« KeyError: 'srv_current_voices' ») et emporte le serveur. On attend donc
    que l'interface web réponde avant de générer."""
    active, port = reglages_web()
    if not active:
        return
    debut, annonce = time.time(), False
    while time.time() - debut < timeout:
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/", timeout=3).close()
            return
        except urllib.error.HTTPError:
            return  # elle répond, même par une erreur
        except (urllib.error.URLError, ConnectionError, OSError):
            pass
        if not annonce:
            info("Attente de l'interface web d'AllTalk…")
            annonce = True
        time.sleep(2)
    info(f"L'interface web d'AllTalk (port {port}) ne répond pas après {timeout} s : génération quand même.")

def preparer_serveur():
    """Rend AllTalk prêt ; renvoie l'objet d'utilisation à fermer après usage."""
    u = Utilisation()
    if not est_pret():
        with VerrouDemarrage():
            if not est_pret():
                if alltalk_en_cours():
                    info("AllTalk est en cours de démarrage : attente…")
                    attendre_pret()
                else:
                    attendre_pret(demarrer())
    attendre_interface_web()
    return u

def liberer_serveur(u, garder):
    """Arrête AllTalk s'il a été lancé par vocalise, sauf --garder ou autre vocalise en cours."""
    pid = pid_vocalise()
    if not garder and pid is not None and u.seul():
        arreter(pid)
    u.fermer()

def commande_arreter():
    pid = pid_vocalise()
    if pid is not None:
        u = Utilisation()
        try:
            if not u.seul():
                sys.exit("Un autre vocalise utilise AllTalk : il l'arrêtera en finissant.")
            arreter(pid)
        finally:
            u.fermer()
        print("AllTalk arrêté.")
    elif est_pret() or alltalk_en_cours():
        print("AllTalk tourne mais n'a pas été lancé par vocalise (interface graphique ou terminal) : laissé tel quel.")
    else:
        print("AllTalk ne tourne pas.")

# ------------------------------------------------------------------ assemblage des wav (sans sox)

def lire_wav(chemin):
    """(octets du chunk fmt, données audio brutes) ; gère les chunks fact/PEAK/LIST d'AllTalk."""
    with open(chemin, "rb") as f:
        b = f.read()
    if b[:4] != b"RIFF" or b[8:12] != b"WAVE":
        raise ValueError(f"{chemin} n'est pas un fichier wav")
    i, fmt, data = 12, None, None
    while i + 8 <= len(b):
        ident, taille = b[i:i + 4], struct.unpack("<I", b[i + 4:i + 8])[0]
        corps = b[i + 8:i + 8 + taille]
        if ident == b"fmt ":
            fmt = corps
        elif ident == b"data":
            data = corps
        i += 8 + taille + (taille & 1)
    if fmt is None or data is None:
        raise ValueError(f"{chemin} : wav incomplet")
    return fmt, data

def assembler(fichiers, sortie):
    """Met bout à bout des wav de même format (ceux d'AllTalk : float 32 bits, 24 kHz, mono)."""
    fmt, morceaux = None, []
    for c in fichiers:
        f, d = lire_wav(c)
        if fmt is None:
            fmt = f
        elif f[:16] != fmt[:16]:
            raise ValueError(f"{c} n'a pas le même format que les autres morceaux")
        morceaux.append(d)
    data = b"".join(morceaux)
    code, canaux, _, _, bloc = struct.unpack("<HHIIH", fmt[:14])
    chunks = [b"fmt ", struct.pack("<I", len(fmt)), fmt]
    if code == 3:  # flottant : chunk fact (nombre de trames) attendu
        chunks += [b"fact", struct.pack("<I", 4), struct.pack("<I", len(data) // max(bloc, 1))]
    chunks += [b"data", struct.pack("<I", len(data)), data]
    if len(data) & 1:
        chunks.append(b"\0")
    corps = b"WAVE" + b"".join(chunks)
    tmp = sortie + ".part"
    with open(tmp, "wb") as f:
        f.write(b"RIFF" + struct.pack("<I", len(corps)) + corps)
    os.replace(tmp, sortie)

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
    # Réponse sans fichier audio (moteur mal configuré, modèle absent…) : échec définitif, inutile
    # de recouper. Sans ce contrôle, l'URL vide menait à la page web d'AllTalk, écrite en « .wav ».
    pas_de_wav = ("AllTalk n'a pas produit de fichier audio (réponse : "
                  + json.dumps(rep, ensure_ascii=False) + f"). Voir {JOURNAL} ; le moteur doit être xtts"
                  + " (engine_loaded dans system/tts_engines/tts_engines.json).")
    chemin = rep.get("output_file_path")
    if not chemin or not os.path.exists(chemin):
        # repli : récupérer le fichier par HTTP
        url = rep.get("output_file_url") or ""
        if not url:
            sys.exit(pas_de_wav)
        if not url.startswith("http"):
            url = API + url
        chemin = tempfile.mktemp(suffix=".wav")
        urllib.request.urlretrieve(url, chemin)
    with open(chemin, "rb") as f:
        entete = f.read(12)
    if entete[:4] != b"RIFF" or entete[8:12] != b"WAVE":
        sys.exit(pas_de_wav)
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
                    help="fichier du dossier voices d'AllTalk ou 'voiceset:nom' (défaut : voiceset:emmanuel s'il existe, sinon emmanuel.wav)")
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
        jeu = os.path.join(ALLTALK, "voices", "xtts_multi_voice_sets", "emmanuel")
        a.voix = "voiceset:emmanuel" if os.path.isdir(jeu) and any(f.endswith(".wav") for f in os.listdir(jeu)) else "emmanuel.wav"
    # Un SIGTERM (fin de session, délai dépassé) passe par les blocs finally comme un Ctrl-C.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    try:
        utilisation = preparer_serveur()
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
        liberer_serveur(utilisation, a.garder)
    sortie = os.path.abspath(a.sortie)
    os.makedirs(os.path.dirname(sortie), exist_ok=True)
    if len(fichiers) == 1:
        shutil.copy(fichiers[0], sortie)
    else:
        assembler(fichiers, sortie)
    print(sortie)

if __name__ == "__main__":
    main()
