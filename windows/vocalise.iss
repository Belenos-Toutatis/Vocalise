; Installateur Windows de Vocalise (Inno Setup 6), produit par .github/workflows/windows.yml :
;   iscc /DVersion=0.3.0 windows\vocalise.iss
; Installation pour l'utilisateur seul (pas de droits administrateur) dans
; %LOCALAPPDATA%\Programs\Vocalise ; ajoute ce dossier au PATH de l'utilisateur pour la commande
; « vocalise ». Les réglages (%APPDATA%\vocalise) et les voix ne sont jamais touchés.

#ifndef Version
  #define Version "0.0.0"
#endif

[Setup]
AppId={{7E1C5B2A-4F7D-4E7B-9C3A-56A1D0B8C1E2}
AppName=Vocalise
AppVersion={#Version}
AppPublisher=Belenos-Toutatis
AppPublisherURL=https://github.com/Belenos-Toutatis/Vocalise
AppUpdatesURL=https://github.com/Belenos-Toutatis/Vocalise/releases
DefaultDirName={localappdata}\Programs\Vocalise
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\gui\target\windows
OutputBaseFilename=vocalise_{#Version}_windows_x64_setup
SetupIconFile=..\gui\assets\vocalise.ico
UninstallDisplayIcon={app}\vocalise-gui.exe
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
ChangesEnvironment=yes
CloseApplications=yes

[Languages]
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "bureau"; Description: "Créer une icône sur le Bureau"; Flags: unchecked
Name: "chemin"; Description: "Ajouter la commande « vocalise » au PATH (pour les scripts et Claude Code)"

[Files]
Source: "..\gui\target\release\vocalise-gui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\vocalise.py"; DestDir: "{app}"; Flags: ignoreversion
Source: "vocalise.cmd"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme

[Icons]
Name: "{userprograms}\Vocalise"; Filename: "{app}\vocalise-gui.exe"
Name: "{userdesktop}\Vocalise"; Filename: "{app}\vocalise-gui.exe"; Tasks: bureau

[Run]
Filename: "{app}\vocalise-gui.exe"; Description: "Lancer Vocalise"; Flags: nowait postinstall skipifsilent

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; \
    ValueData: "{olddata};{app}"; Tasks: chemin; Check: PasDansLePath(ExpandConstant('{app}'))

[Code]
function PasDansLePath(Dossier: string): Boolean;
var
  Actuel: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Actuel) then
    Result := True
  else
    Result := Pos(';' + Uppercase(Dossier) + ';', ';' + Uppercase(Actuel) + ';') = 0;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Actuel, Dossier: string;
  P: Integer;
begin
  if CurUninstallStep <> usPostUninstall then
    Exit;
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Actuel) then
    Exit;
  Dossier := ExpandConstant('{app}');
  Actuel := ';' + Actuel + ';';
  P := Pos(';' + Uppercase(Dossier) + ';', Uppercase(Actuel));
  if P = 0 then
    Exit;
  Delete(Actuel, P, Length(Dossier) + 1);
  Actuel := Copy(Actuel, 2, Length(Actuel) - 2);
  RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Actuel);
end;
