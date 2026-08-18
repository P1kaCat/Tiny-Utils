@echo off
setlocal
title Installation de Tiny Utils

echo ======================================================
echo           TINY UTILS - MOD TINY GLADE
echo     Multijoueur + Zone Illimitee + Menu In-Game
echo ======================================================
echo.

echo [INFO] Suppression de l'ancien build...
if exist "gladesync\target\release\dxgi.dll" del /Q "gladesync\target\release\dxgi.dll"
if exist "dxgi.dll" del /Q "dxgi.dll"

echo [INFO] Recompilation de Tiny Utils en cours...
cd gladesync
cargo build --release
if errorlevel 1 (
    cd ..
    echo.
    echo ======================================================
    echo   [ERREUR] La compilation a echoue !
    echo   Verifie que tu as fait git pull pour recuperer les
    echo   derniers fixes.
    echo ======================================================
    echo.
    pause
    exit /b 1
)
cd ..

if exist "gladesync\target\release\dxgi.dll" (
    copy /Y "gladesync\target\release\dxgi.dll" "dxgi.dll" >nul
    echo.
    echo ======================================================
    echo   [SUCCES] Tiny Utils a ete installe avec succes !
    echo ======================================================
    echo.
    echo 1. Lance "tiny-glade.exe".
    echo 2. Le menu "Tiny Utils" s'ouvrira automatiquement in-game.
    echo 3. Pour heberger : entre le port et clique "Heberger".
    echo 4. Pour rejoindre : entre l'IP et le port de ton ami.
    echo.
) else (
    echo [ERREUR] dxgi.dll introuvable apres compilation.
)

echo.
pause
