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

if not exist "%GAME_DIR%\dxgi.dll" (
    echo [INFO] dxgi.dll introuvable. Le mod n'est pas installe ici.
    echo.
    pause
    exit /b 0
)

echo [INFO] dxgi.dll trouve. Tentative de suppression...

REM -- Try normal delete --
del /F /Q "%GAME_DIR%\dxgi.dll" 2>nul
if not exist "%GAME_DIR%\dxgi.dll" goto :success

REM -- Try with attrib reset --
echo [INFO] Retry avec reset attributs...
attrib -R -S -H "%GAME_DIR%\dxgi.dll" 2>nul
del /F /Q "%GAME_DIR%\dxgi.dll" 2>nul
if not exist "%GAME_DIR%\dxgi.dll" goto :success

REM -- Try with PowerShell --
echo [INFO] Retry avec PowerShell...
powershell -NoProfile -Command "Remove-Item -Force '%GAME_DIR%\dxgi.dll'" 2>nul
if not exist "%GAME_DIR%\dxgi.dll" goto :success

REM -- Need admin --
echo [INFO] Tentative avec droits admin...
powershell -NoProfile -Command "Start-Process cmd -ArgumentList '/c del /F /Q \"%GAME_DIR%\dxgi.dll\"' -Verb RunAs -Wait" 2>nul
if not exist "%GAME_DIR%\dxgi.dll" goto :success

echo.
echo ======================================================
echo   [ERREUR] Impossible de supprimer dxgi.dll
echo ======================================================
echo.
echo  Tu peux le supprimer manuellement :
echo  1. Ferme Tiny Glade (Gestionnaire des taches)
echo  2. Va dans : %GAME_DIR%
echo  3. Supprime dxgi.dll a la main
echo.
pause
exit /b 1

:success
echo.
echo ======================================================
echo   [OK] Tiny Utils desinstalle !
echo   Le jeu est revenu en version originale (Vanilla).
echo ======================================================
echo.
pause
