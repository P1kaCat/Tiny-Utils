@echo off
setlocal enabledelayedexpansion
title Tiny Utils - Installateur Auto
chcp 65001 >nul 2>&1

echo ======================================================
echo         TINY UTILS - INSTALLATEUR AUTOMATIQUE
echo     Telechargement + Compilation + Installation
echo ======================================================
echo.

REM ── Config ──
set REPO_URL=https://github.com/P1kaCat/Tiny-Utils.git
set REPO_ZIP=https://github.com/P1kaCat/Tiny-Utils/archive/refs/heads/master.zip
set MOD_FOLDER=gladesync
set OUTPUT_DLL=dxgi.dll

REM ── Detect game folder ──
if exist "%CD%\tiny-glade.exe" (
    set GAME_DIR=%CD%
    echo [INFO] Dossier du jeu detecte : %CD%
    goto :found_game
)

if exist "%CD%\gladesync\Cargo.toml" (
    set GAME_DIR=%CD%
    echo [INFO] Dossier du jeu detecte (gladesync present) : %CD%
    goto :found_game
)

echo Le dossier du jeu n'a pas ete detecte automatiquement.
echo Place cet installateur dans le dossier de Tiny Glade (la ou se trouve tiny-glade.exe)
echo ou indique le chemin ci-dessous.
echo.
set /p GAME_DIR="Chemin du dossier Tiny Glade (ou appuye sur Entree pour le dossier courant) : "
if "!GAME_DIR!"=="" set GAME_DIR=%CD%

:found_game
echo.

REM ── Check Rust / Cargo ──
where cargo >nul 2>&1
if errorlevel 1 (
    echo [ERREUR] Cargo (Rust) n'est pas installe ou pas dans le PATH.
    echo.
    echo Installe Rust depuis : https://rustup.rs
    echo Telecharge rustup-init.exe, lance-le, puis relance cet installateur.
    echo.
    pause
    exit /b 1
)
echo [OK] Cargo detecte.

REM ── Download / Update source ──
echo.
echo [INFO] Telechargement des fichiers depuis GitHub...

where git >nul 2>&1
if not errorlevel 1 (
    if exist "%GAME_DIR%\.git" (
        echo [INFO] Repository git detecte, git pull...
        cd /d "%GAME_DIR%"
        git pull origin master 2>nul
        if errorlevel 1 (
            echo [WARN] git pull a echoue, tentative avec git clone...
            goto :clone_method
        )
        echo [OK] Code source mis a jour.
        goto :build
    ) else (
        :clone_method
        echo [INFO] Clonage du repository...
        cd /d "%GAME_DIR%"
        git clone --depth 1 "%REPO_URL%" "%TEMP%\tiny-utils-tmp" 2>nul
        if errorlevel 1 (
            echo [WARN] git clone echoue, fallback methode zip...
            goto :zip_method
        )
        if exist "%TEMP%\tiny-utils-tmp\gladesync" (
            xcopy /E /I /Y "%TEMP%\tiny-utils-tmp\gladesync" "%GAME_DIR%\gladesync" >nul 2>&1
            rd /S /Q "%TEMP%\tiny-utils-tmp" 2>nul
        )
        echo [OK] Code source telecharge.
        goto :build
    )
)

:zip_method
echo [INFO] Telechargement via PowerShell (pas de git)...
set ZIP_PATH=%TEMP%\tiny-utils-master.zip
set EXTRACT_PATH=%TEMP%\tiny-utils-extract

powershell -NoProfile -Command "try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '%REPO_ZIP%' -OutFile '%ZIP_PATH%' -UseBasicParsing } catch { exit 1 }"
if errorlevel 1 (
    echo [ERREUR] Impossible de telecharger les fichiers depuis GitHub.
    echo Verifie ta connexion internet.
    echo.
    pause
    exit /b 1
)

echo [INFO] Extraction...
if exist "%EXTRACT_PATH%" rd /S /Q "%EXTRACT_PATH%"
powershell -NoProfile -Command "Expand-Archive -Path '%ZIP_PATH%' -DestinationPath '%EXTRACT_PATH%' -Force"
if not exist "%EXTRACT_PATH%\Tiny-Utils-master\gladesync" (
    echo [ERREUR] Extraction echouee.
    pause
    exit /b 1
)

xcopy /E /I /Y "%EXTRACT_PATH%\Tiny-Utils-master\gladesync" "%GAME_DIR%\gladesync" >nul 2>&1
rd /S /Q "%EXTRACT_PATH%" 2>nul
del /Q "%ZIP_PATH" 2>nul
echo [OK] Code source telecharge.

:build
echo.
echo [INFO] Suppression de l'ancien build...
if exist "%GAME_DIR%\gladesync\target\release\dxgi.dll" del /Q "%GAME_DIR%\gladesync\target\release\dxgi.dll"
if exist "%GAME_DIR%\dxgi.dll" del /Q "%GAME_DIR%\dxgi.dll"

echo [INFO] Compilation en cours (cela peut prendre quelques minutes)...
cd /d "%GAME_DIR%\gladesync"
cargo build --release
if errorlevel 1 (
    echo.
    echo ======================================================
    echo   [ERREUR] La compilation a echoue !
    echo   Verifie que Rust est a jour : rustup update
    echo ======================================================
    echo.
    pause
    exit /b 1
)

cd /d "%GAME_DIR%"
if exist "gladesync\target\release\dxgi.dll" (
    copy /Y "gladesync\target\release\dxgi.dll" "dxgi.dll" >nul
    echo.
    echo ======================================================
    echo   [SUCCES] Tiny Utils installe avec succes !
    echo ======================================================
    echo.
    echo  Le fichier dxgi.dll a ete place dans :
    echo   %GAME_DIR%
    echo.
    echo  1. Lance tiny-glade.exe
    echo  2. Le menu Tiny Utils s'ouvre in-game
    echo  3. Host : entre un port ^> Start Hosting
    echo  4. Join : entre l'IP + port ^> Join
    echo.
    echo  Pour mettre a jour plus tard : relance easy_install.bat
    echo  Pour desinstaller : lance uninstall_mod.bat
    echo.
) else (
    echo [ERREUR] dxgi.dll introuvable apres compilation.
    echo Le build a peut-etre echoue silencieusement.
    echo.
)

pause
