@echo off
rem Commande « vocalise » sous Windows : lance vocalise.py (installé à côté de ce fichier).
rem Python : lanceur py, sinon python, sinon celui de l'environnement d'AllTalk.
rem Pas de blocs entre parenthèses autour des appels : %errorlevel% y serait lu avant l'appel
rem (toujours 0), et un échec de vocalise passerait pour une réussite.
setlocal
set "SCRIPT=%~dp0vocalise.py"
where py >nul 2>nul && goto py
where python >nul 2>nul && goto python
set "PYALLTALK=%USERPROFILE%\alltalk_tts\alltalk_environment\env\python.exe"
if defined ALLTALK_DIR set "PYALLTALK=%ALLTALK_DIR%\alltalk_environment\env\python.exe"
if exist "%PYALLTALK%" goto alltalk
echo Python est introuvable : installez-le (winget install Python.Python.3.12) ou AllTalk. 1>&2
exit /b 1

:py
py -3 "%SCRIPT%" %*
exit /b %errorlevel%

:python
python "%SCRIPT%" %*
exit /b %errorlevel%

:alltalk
"%PYALLTALK%" "%SCRIPT%" %*
exit /b %errorlevel%
