@echo off
echo ========================================
echo   Building Xvpn Standalone Client
echo ========================================
echo.

cargo build --release --bin Xvpn

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] Build failed!
    pause
    exit /b 1
)

echo.
echo Copying to project root...
copy /Y "target\release\Xvpn.exe" "Xvpn.exe" >nul

echo.
echo ========================================
echo   Build Complete!
echo ========================================
echo.
echo   Output: Xvpn.exe
echo   Size:
for %%A in (Xvpn.exe) do echo     %%~zA bytes
echo.
echo   Just double-click Xvpn.exe to connect!
echo   (It will auto-request admin privileges)
echo.
pause
