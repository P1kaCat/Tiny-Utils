@echo off
setlocal enabledelayedexpansion
title Tiny Utils - Installateur Auto
chcp 65001 >nul 2>&1

echo ======================================================
echo         TINY UTILS - INSTALLATEUR AUTOMATIQUE
echo       Telechargement direct, aucune dependance
echo ======================================================
echo.

set DLL_URL=https://github.com/P1kaCat/Tiny-Utils/releases/latest/download/dxgi.dll

REM -- Detect game folder --
if exist "%CD%\tiny-glade.exe" (
    set GAME_DIR=%CD%
    echo [INFO] Dossier du jeu detecte : %CD%
    goto :found_game
)

if exist "%CD%\dxgi.dll" (
    set GAME_DIR=%CD%
    echo [INFO] Dossier detecte (dxgi.dll present) : %CD%
    goto :found_game
)

echo Le dossier du jeu n'a pas ete detecte automatiquement.
echo Place cet installateur dans le dossier de Tiny Glade (la ou se trouve tiny-glade.exe).
echo.
set /p GAME_DIR="Ou indique le chemin (Entree = dossier courant) : "
if "!GAME_DIR!"=="" set GAME_DIR=%CD%

:found_game
echo.

REM -- Delete old mod --
if exist "%GAME_DIR%\dxgi.dll" (
    echo [INFO] Suppression de l'ancien dxgi.dll...
    del /Q "%GAME_DIR%\dxgi.dll"
)

REM -- Download pre-built DLL --
echo [INFO] Telechargement du mod depuis GitHub...

powershell -NoProfile -Command "try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '%DLL_URL%' -OutFile '%GAME_DIR%\dxgi.dll' -UseBasicParsing } catch { Write-Host $_.Exception.Message; exit 1 }"

if errorlevel 1 (
    echo.
    echo ======================================================
    echo   [ERREUR] Telechargement echoue.
    echo   Verifie ta connexion internet.
    echo   Si le probleme persiste, le build GitHub Actions
    echo   n'est peut-etre pas encore pret. Reessaie dans
    echo   2-3 minutes.
    echo ======================================================
    echo.
    pause
    exit /b 1
)

if not exist "%GAME_DIR%\dxgi.dll" (
    echo [ERREUR] Fichier non telecharge.
    pause
    exit /b 1
)

echo.
echo ======================================================
echo   [SUCCES] Tiny Utils installe !
echo ======================================================
echo.
echo  dxgi.dll place dans : %GAME_DIR%
echo.
echo  1. Lance tiny-glade.exe
echo  2. Le menu Tiny Utils s'ouvre in-game
echo  3. Host : port ^> Start Hosting
echo  4. Join : IP + port ^> Join
echo.
echo  Pour desinstaller : lance uninstall_mod.bat
echo.

pause
