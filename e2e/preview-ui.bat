@echo off
chcp 65001 >nul
cd /d "%~dp0"
rem Opens one window of this build for a look, without the test: no typing,
rem no Python. Settings go to a temporary folder, not yours.
if not exist "%~dp0righttype.exe" (
  echo  righttype.exe is missing: unzip the whole self-test zip first.
  pause
  exit /b 1
)
echo ============================================================
echo  ดูหน้าตา RightType (ไม่ต้องรันทดสอบ) / Preview a window
echo ============================================================
echo.
echo   1  Palette (Ctrl+Shift+Space)
echo   2  Settings - แอป / Apps
echo   3  Settings - ข้อความสำเร็จรูป / Snippets
echo   4  Settings - ทั่วไป / General
echo   5  ซ่อมข้อความ / Fix text
echo   6  แผนผังแป้นพิมพ์ / Keyboard map
echo   7  Settings - แป้นพิมพ์ / Keyboard
echo.
echo  RightType ที่เปิดอยู่จะถูกปิดระหว่างดู ปิดหน้าต่างนี้แล้วเปิดใหม่ได้เลย
echo.
set /p pick="เลือก / pick 1-7: "
set "show=palette"
if "%pick%"=="2" set "show=settings-blocked"
if "%pick%"=="3" set "show=settings-snippets"
if "%pick%"=="4" set "show=settings"
if "%pick%"=="5" set "show=fixer"
if "%pick%"=="6" set "show=keymap"
if "%pick%"=="7" set "show=settings-keyboard"
set "APPDATA=%TEMP%\RightType-preview"
if not exist "%APPDATA%\RightType" mkdir "%APPDATA%\RightType"
if not exist "%APPDATA%\RightType\config.toml" (
  >"%APPDATA%\RightType\config.toml" echo onboarded = true
)
set "RIGHTTYPE_SHOW=%show%"
start "" "%~dp0righttype.exe"
echo.
echo  เปิดแล้ว ปิด RightType จากไอคอนที่ถาดเมื่อดูเสร็จ
echo  Opened. Quit it from the tray icon when done.
timeout /t 5 >nul
