@echo off
setlocal enabledelayedexpansion

cd /d "%~dp0\.."

set TARGETS=x86_64-unknown-linux-musl aarch64-unknown-linux-musl armv7-unknown-linux-musleabihf x86_64-pc-windows-gnu

where cross >nul 2>nul
if errorlevel 1 (
    echo [ОШИБКА] cross не найден. Установи его:
    echo   cargo install cross --git https://github.com/cross-rs/cross
    exit /b 1
)

docker info >nul 2>nul
if errorlevel 1 (
    echo [ОШИБКА] Docker недоступен. Убедись, что Docker Desktop запущен.
    exit /b 1
)

if not exist dist mkdir dist

for %%T in (%TARGETS%) do (
    echo.
    echo === Сборка под %%T ===
    cross build --release --target %%T -p hGB-server -p hGB-client
    if errorlevel 1 (
        echo [ОШИБКА] Сборка под %%T упала, смотри вывод выше.
        exit /b 1
    )

    set "OUT=dist\%%T"
    if not exist "!OUT!" mkdir "!OUT!"

    echo %%T | findstr /C:"windows" >nul
    if !errorlevel! == 0 (
        copy /Y "target\%%T\release\hGB-server.exe" "!OUT!\" >nul
        copy /Y "target\%%T\release\hGB-client.exe" "!OUT!\" >nul
    ) else (
        copy /Y "target\%%T\release\hGB-server" "!OUT!\" >nul
        copy /Y "target\%%T\release\hGB-client" "!OUT!\" >nul
    )

    copy /Y "hGB-server\config.example.yaml" "!OUT!\server.config.example.yaml" >nul
    copy /Y "hGB-client\config.example.yaml" "!OUT!\client.config.example.yaml" >nul

    echo   -^> !OUT!
)

echo.
echo Done. Binares in dist\:
dir /s /b dist\*.exe dist\hGB-server dist\hGB-client 2>nul

endlocal
