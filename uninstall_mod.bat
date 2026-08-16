@echo off
setlocal enabledelayedexpansion
title Tiny Utils - Desinstallation
chcp 65001 >nul 2>&1

echo ======================================================
echo           TINY UTILS - DESINSTALLATION
echo ======================================================
echo.

REM -- Detect game folder --
if exist "%CD%\tiny-glade.exe" (
    set GAME_DIR=%CD%
    goto :found_game
)

if exist "%CD%\dxgi.dll" (
    set GAME_DIR=%CD%
    goto :found_game
)

echo Dossier du jeu non detecte.
echo Place ce fichier dans le dossier de Tiny Glade.
echo.
set /p GAME_DIR="Ou indique le chemin (Entree = dossier courant) : "
if "!GAME_DIR!"=="" set GAME_DIR=%CD%

:found_game
echo.
echo [INFO] Dossier : %GAME_DIR%
echo.

if exist "%GAME_DIR%\dxgi.dll" (
    del /F /Q "%GAME_DIR%\dxgi.dll" 2>nul
    if exist "%GAME_DIR%\dxgi.dll" (
        echo [ERREUR] Impossible de supprimer dxgi.dll.
        echo Le jeu est peut-etre encore lance. Ferme Tiny Glade puis reessaie.
    ) else (
        echo [OK] Tiny Utils a ete desinstalle avec succes.
        echo Le jeu est maintenant revenu en version originale (Vanilla).
    )
) else (
    echo [INFO] dxgi.dll introuvable. Tiny Utils n'est pas installe ici.
)

echo.
pause
