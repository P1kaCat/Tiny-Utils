@echo off
setlocal
title Desinstallation de Tiny Utils

echo ======================================================
echo           TINY UTILS - DESINSTALLATION
echo ======================================================
echo.

if exist "dxgi.dll" (
    del /F /Q "dxgi.dll"
    echo [OK] Tiny Utils a ete desinstalle avec succes.
    echo Le jeu est maintenant revenu en version originale (Vanilla).
) else (
    echo [INFO] Tiny Utils n'est pas installe dans ce dossier.
)

echo.
pause
