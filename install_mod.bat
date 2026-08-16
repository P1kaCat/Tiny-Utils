@echo off
setlocal
title Installation de Tiny Utils

echo ======================================================
echo           TINY UTILS - MOD TINY GLADE
echo     Multijoueur + Zone Illimitee + Menu In-Game
echo ======================================================
echo.

echo [INFO] Recompilation de Tiny Utils en cours...
cd gladesync
cargo build --release
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
    echo [ERREUR] Impossible de compiler dxgi.dll.
    echo Verifie que Rust/Cargo est installe sur ta machine.
)

echo.
pause
