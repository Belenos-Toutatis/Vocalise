@echo off
rem Commande « vocalise » sous Windows : lance vocalise.py (installé à côté de ce fichier).
rem Python : lanceur py, sinon python, sinon celui de l'environnement d'AllTalk.
setlocal
set "SCRIPT=%~dp0vocalise.py"
where py >nul 2>nul
if not errorlevel 1 (
    py -3 "%SCRIPT%" %*
    exit /b %errorlevel%
)
where python >nul 2>nul
if not errorlevel 1 (
    python "%SCRIPT%" %*
    exit /b %errorlevel%
)
set "PYALLTALK=%USERPROFILE%\alltalk_tts\alltalk_environment\env\python.exe"
if defined ALLTALK_DIR set "PYALLTALK=%ALLTALK_DIR%\alltalk_environment\env\python.exe"
if exist "%PYALLTALK%" (
    "%PYALLTALK%" "%SCRIPT%" %*
    exit /b %errorlevel%
)
echo Python est introuvable : installez-le (winget install Python.Python.3.12) ou AllTalk. 1>&2
exit /b 1
