@echo off
setlocal

:: ==============================================================================
:: PROJEKT: Configurable CSV Analyzer (configurable_csv_analyzer)
:: DATEI:   start-configurable-csv-analyzer.bat
:: ZWECK:   Automatischer Build (Release) und Start der Desktop-Anwendung
:: ==============================================================================

title Configurable CSV Analyzer
set "PROJECT_NAME=Configurable CSV Analyzer"
set "BINARY_NAME=configurable_csv_analyzer"

cd /d "%~dp0"

echo [%PROJECT_NAME%] Initialisiere Umgebung...

:: Unterdruecke rustup-Warnungen zu nicht-kanonischen Pfaden auf virtuellen/ImDisk-Laufwerken (z.B. R:\)
if not defined RUSTUP_LOG set "RUSTUP_LOG=error"

:: Cargo / Rust Pfad sicherstellen (unterstuetzt R:\.cargo\bin sowie Userprofile)
where cargo >nul 2>&1
if errorlevel 1 (
    if defined CARGO_HOME (
        if exist "%CARGO_HOME%\bin\cargo.exe" set "PATH=%CARGO_HOME%\bin;%PATH%"
    )
    if exist "R:\.cargo\bin\cargo.exe" (
        set "PATH=R:\.cargo\bin;%PATH%"
    ) else if exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
        set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
    )
)

:: ImDisk/RAM-Disk Fix fuer Cargo Registry:
:: Cargo (tar-Crate) ruft beim Entpacken std::fs::canonicalize() auf. Auf ImDisk-Laufwerken (R:\)
:: schlaegt dies mit "Unzulaessige Funktion (os error 1)" fehl. Eine NTFS-Junction nach
:: %USERPROFILE%\.cargo\registry behebt den Fehler und haelt den Crate-Cache ueber Neustarts persistent.
if exist "R:\.cargo" (
    if not exist "%USERPROFILE%\.cargo\registry" (
        mkdir "%USERPROFILE%\.cargo\registry" 2>nul
    )
    fsutil reparsepoint query "R:\.cargo\registry" >nul 2>&1
    if errorlevel 1 (
        if exist "R:\.cargo\registry" (
            rmdir /s /q "R:\.cargo\registry" 2>nul
        )
        mklink /J "R:\.cargo\registry" "%USERPROFILE%\.cargo\registry" >nul 2>&1
    )
)

:: Abhaengigkeit slint-desktop-kit absichern (insbesondere auf RAM-Disk R:\git)
if not exist "..\slint-desktop-kit\Cargo.toml" (
    if exist "%USERPROFILE%\Projekte\slint-desktop-kit\Cargo.toml" (
        echo [%PROJECT_NAME%] Verlinke Abhaengigkeit slint-desktop-kit aus %USERPROFILE%\Projekte...
        mklink /J "..\slint-desktop-kit" "%USERPROFILE%\Projekte\slint-desktop-kit" >nul
    ) else if exist "%USERPROFILE%\git\slint-desktop-kit\Cargo.toml" (
        echo [%PROJECT_NAME%] Verlinke Abhaengigkeit slint-desktop-kit aus %USERPROFILE%\git...
        mklink /J "..\slint-desktop-kit" "%USERPROFILE%\git\slint-desktop-kit" >nul
    ) else (
        echo [%PROJECT_NAME%] Klone fehlende Abhaengigkeit slint-desktop-kit nach ..\slint-desktop-kit...
        git clone https://github.com/1337userxyz/slint-desktop-kit.git "..\slint-desktop-kit"
        if errorlevel 1 (
            echo [%PROJECT_NAME%] [FEHLER] Konnte slint-desktop-kit nicht klonen!
            pause
            exit /b 1
        )
    )
)

:: Automatischer Vorab-Clean von ungenutzten Build-Artefakten (schont RAM-Disk R:\)
:: 1. target\debug entfernen falls vorhanden (spart > 1 GB)
if exist "%~dp0target\debug" (
    echo [%PROJECT_NAME%] Bereinige ungenutzte Debug-Builddaten [target\debug]...
    rmdir /s /q "%~dp0target\debug" 2>nul
)

:: 2. Verwaiste PDBs und alte eigene Hash-Artefakte in deps aufraeumen
if exist "%~dp0target\release\deps\%BINARY_NAME%-*.pdb" (
    del /f /q "%~dp0target\release\deps\%BINARY_NAME%-*.pdb" 2>nul
)

:: 3. Parameter --clean unterstuetzen (erzwingt bei Bedarf vollen Clean-Build)
if /i "%~1"=="--clean" (
    echo [%PROJECT_NAME%] Schalter --clean erkannt: Bereinige target vollstaendig...
    cargo clean
    shift
)

set "TARGET_EXE=%~dp0target\release\%BINARY_NAME%.exe"

:: Cargo prueft Fingerprints & Slint-Aenderungen vollautomatisch.
:: Wenn nichts geaendert wurde, ueberspringt Cargo den Build in ca. 0.1 Sekunden.
echo [%PROJECT_NAME%] Pruefe / Baue Release-Binary...
cargo build --release --bin %BINARY_NAME%
if errorlevel 1 (
    echo.
    echo [%PROJECT_NAME%] [FEHLER] Kompilierung fehlgeschlagen.
    echo Falls der Fehler "linker link.exe not found" lautet, starte bitte:
    echo   R:\git\pwsh7-scripts\Setup\DevWorkstation\Apps\VisualStudioBuildTools\Install-VisualStudioBuildTools.bat
    pause
    exit /b 1
)

:: Fertiges Programm starten
echo [%PROJECT_NAME%] Starte %BINARY_NAME%...
start "" "%TARGET_EXE%" %*
