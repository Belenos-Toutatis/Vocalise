//! Ce qui dépend du système d'exploitation (Linux ou Windows), regroupé ici pour que le reste du
//! code reste commun : dossiers de l'utilisateur, lanceur d'AllTalk, ouverture d'un fichier,
//! recherche d'un programme, commandes d'installation proposées par le diagnostic.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Vrai sous Windows.
pub const WINDOWS: bool = cfg!(windows);

/// Dossier personnel : `$HOME` (Linux), `%USERPROFILE%` (Windows).
pub fn dossier_personnel() -> PathBuf {
    let var = if WINDOWS { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Dossier des configurations : `$XDG_CONFIG_HOME` ou `~/.config` (Linux), `%APPDATA%` (Windows).
pub fn dossier_config() -> PathBuf {
    if WINDOWS {
        if let Some(d) = std::env::var_os("APPDATA").filter(|d| !d.is_empty()) {
            return PathBuf::from(d);
        }
        return dossier_personnel().join("AppData").join("Roaming");
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| dossier_personnel().join(".config"))
}

/// Nom du lanceur fourni par AllTalk à la racine de son dossier.
pub const LANCEUR_ALLTALK: &str = if WINDOWS { "start_alltalk.bat" } else { "start_alltalk.sh" };

/// Commande sans fenêtre de console sous Windows (sans effet sous Linux).
pub fn commande(programme: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(programme);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
pub const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// Un programme est-il dans le PATH ? Sous Windows, les extensions de `PATHEXT` sont essayées.
#[cfg_attr(windows, allow(dead_code))]
pub fn dans_le_path(programme: &str) -> bool {
    if programme.is_empty() {
        return false;
    }
    let Some(chemins) = std::env::var_os("PATH") else { return false };
    let extensions: Vec<String> = if WINDOWS {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into());
        std::iter::once(String::new()).chain(pathext.split(';').filter(|e| !e.is_empty()).map(str::to_string)).collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&chemins).any(|d| extensions.iter().any(|ext| executable(&d.join(format!("{programme}{ext}")))))
}

#[cfg(unix)]
fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(not(unix))]
#[allow(dead_code)]
fn executable(p: &Path) -> bool {
    p.is_file()
}

/// Ouvre un chemin ou une URL avec l'application par défaut, sans bloquer ; échec silencieux.
/// Linux : `xdg-open`. Windows : `explorer` (dossiers, fichiers et adresses web).
pub fn ouvrir(cible: &std::ffi::OsStr) {
    use std::process::Stdio;
    let programme = if WINDOWS { "explorer" } else { "xdg-open" };
    if let Ok(mut child) = commande(programme)
        .arg(cible)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Commandes d'installation proposées par le diagnostic, à coller dans un terminal
/// (Ubuntu/Debian) ou dans PowerShell (Windows). `None` : rien à installer en ligne de commande.
pub struct Installations {
    pub alltalk: String,
    pub pilote_nvidia: Option<&'static str>,
    pub ffmpeg: &'static str,
}

pub fn installations() -> Installations {
    if WINDOWS {
        Installations {
            alltalk: r"git clone -b alltalkbeta https://github.com/erew123/alltalk_tts $env:USERPROFILE\alltalk_tts; cd $env:USERPROFILE\alltalk_tts; .\atsetup.bat".into(),
            // Le pilote NVIDIA s'installe depuis le site du constructeur ; pas de commande.
            pilote_nvidia: None,
            ffmpeg: "winget install --id Gyan.FFmpeg -e",
        }
    } else {
        Installations {
            alltalk: "git clone -b alltalkbeta https://github.com/erew123/alltalk_tts ~/alltalk_tts && cd ~/alltalk_tts && ./atsetup.sh".into(),
            pilote_nvidia: Some("sudo ubuntu-drivers install"),
            ffmpeg: "sudo apt install ffmpeg",
        }
    }
}

/// Fichier joint aux publications GitHub qui installe Vocalise sur ce système.
pub fn est_paquet(nom: &str) -> bool {
    if WINDOWS {
        nom.to_ascii_lowercase().ends_with("_setup.exe")
    } else {
        nom.ends_with(".deb")
    }
}

/// Bouton de téléchargement et consigne d'installation d'une nouvelle version.
pub const BOUTON_PAQUET: &str = if WINDOWS { "Télécharger l'installateur" } else { "Télécharger le paquet .deb" };
pub const CONSIGNE_PAQUET: &str = if WINDOWS {
    "Téléchargez l'installateur, fermez Vocalise puis lancez-le : vos réglages et vos voix sont conservés."
} else {
    "Téléchargez le paquet puis installez-le : sudo apt install ./vocalise_<version>_amd64.deb"
};

/// Où coller une commande : « un terminal » ou « PowerShell ».
pub const TERMINAL: &str = if WINDOWS { "PowerShell" } else { "un terminal" };

/// Raccourci pour coller dans ce terminal.
pub const COLLER: &str = if WINDOWS { "Ctrl+V" } else { "Ctrl+Maj+V" };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dossiers_coherents() {
        assert!(dossier_config().is_absolute() || dossier_config().starts_with("."));
        assert!(LANCEUR_ALLTALK.starts_with("start_alltalk."));
        assert!(!dans_le_path(""));
        assert!(!dans_le_path("programme-qui-n-existe-vraiment-pas-1234"));
        // Un programme présent sur les deux systèmes.
        assert!(dans_le_path(if WINDOWS { "cmd" } else { "sh" }));
    }
}
