@echo off
setlocal EnableExtensions EnableDelayedExpansion

cd /d "%~dp0\.."

set "BIN_NAME=realraw"
set "ACTION=%~1"
if "%ACTION%"=="" set "ACTION=help"

set "VERSION="
for /f "usebackq tokens=3 delims= " %%V in (`findstr /b /c:"version = " Cargo.toml`) do (
  set "VERSION=%%~V"
  goto :version_done
)
:version_done
if not defined VERSION set "VERSION=0.1.0"
set "VERSION=%VERSION:"=%"

if /i "%ACTION%"=="help" goto :usage
if /i "%ACTION%"=="--help" goto :usage
if /i "%ACTION%"=="-h" goto :usage
if /i "%ACTION%"=="exe" goto :cmd_exe
if /i "%ACTION%"=="nsis" goto :cmd_nsis
if /i "%ACTION%"=="wix" goto :cmd_wix
if /i "%ACTION%"=="all" goto :cmd_all

echo unknown command: %ACTION%
goto :usage

:usage
echo Usage: scripts\package.bat ^<command^>
echo.
echo Windows-only packaging commands:
echo   exe     Build release .exe (icon embedded via build.rs)
echo   nsis    Build NSIS installer (.exe setup)
echo   wix     Build WiX MSI installer
echo   all     Build exe, then NSIS and WiX if tools are available
echo   help    Show this help
exit /b 0

:ensure_release_bin
if exist "target\release\%BIN_NAME%.exe" (
  echo ==^> Using existing target\release\%BIN_NAME%.exe
  exit /b 0
)
echo ==^> Building release binary...
where cargo >nul 2>&1
if errorlevel 1 (
  echo error: 'cargo' is required but not installed.
  exit /b 1
)
cargo build --release
exit /b 0

:cmd_exe
call :ensure_release_bin
if errorlevel 1 exit /b 1
echo ==^> Done: target\release\%BIN_NAME%.exe
exit /b 0

:find_tool
set "TOOL_NAME=%~1"
set "TOOL_PATH="

REM 1. PATH
where "%TOOL_NAME%" >nul 2>&1
if not errorlevel 1 (
  for /f "delims=" %%P in ('where "%TOOL_NAME%"') do (
    set "TOOL_PATH=%%P"
    goto :eof
  )
)

REM 2. Scoop shims and app dirs
if exist "%USERPROFILE%\scoop\shims\%TOOL_NAME%.exe" set "TOOL_PATH=%USERPROFILE%\scoop\shims\%TOOL_NAME%.exe" & goto :eof
if /i "%TOOL_NAME%"=="makensis" (
  if exist "%USERPROFILE%\scoop\apps\nsis\current\makensis.exe" set "TOOL_PATH=%USERPROFILE%\scoop\apps\nsis\current\makensis.exe" & goto :eof
) else (
  if exist "%USERPROFILE%\scoop\apps\wixtoolset\current\bin\%TOOL_NAME%.exe" set "TOOL_PATH=%USERPROFILE%\scoop\apps\wixtoolset\current\bin\%TOOL_NAME%.exe" & goto :eof
)

REM 3. CI portable tools (under repo tools/)
if /i "%TOOL_NAME%"=="makensis" (
  for /d %%D in ("%~dp0..\tools\nsis-*") do if exist "%%D\makensis.exe" set "TOOL_PATH=%%D\makensis.exe" & goto :eof
  if exist "%~dp0..\tools\nsis\makensis.exe" set "TOOL_PATH=%~dp0..\tools\nsis\makensis.exe" & goto :eof
)
if defined GITHUB_WORKSPACE if exist "%GITHUB_WORKSPACE%\tools\wix\%TOOL_NAME%.exe" set "TOOL_PATH=%GITHUB_WORKSPACE%\tools\wix\%TOOL_NAME%.exe" & goto :eof
if exist "%~dp0..\tools\wix\%TOOL_NAME%.exe" set "TOOL_PATH=%~dp0..\tools\wix\%TOOL_NAME%.exe" & goto :eof

REM 4. NSIS traditional install
if /i "%TOOL_NAME%"=="makensis" (
  if exist "%ProgramFiles(x86)%\NSIS\makensis.exe" set "TOOL_PATH=%ProgramFiles(x86)%\NSIS\makensis.exe" & goto :eof
  if exist "%ProgramFiles%\NSIS\makensis.exe" set "TOOL_PATH=%ProgramFiles%\NSIS\makensis.exe" & goto :eof
)

REM 5. WiX Toolset v3 MSI install
for /d %%D in ("%ProgramFiles(x86)%\WiX Toolset v*") do if exist "%%D\bin\%TOOL_NAME%.exe" set "TOOL_PATH=%%D\bin\%TOOL_NAME%.exe" & goto :eof
for /d %%D in ("%ProgramFiles%\WiX Toolset v*") do if exist "%%D\bin\%TOOL_NAME%.exe" set "TOOL_PATH=%%D\bin\%TOOL_NAME%.exe" & goto :eof

goto :eof

:cmd_nsis
call :find_tool makensis
if not defined TOOL_PATH (
  echo error: 'makensis' is required but not installed.
  echo install with:
  echo   scoop bucket add extras
  echo   scoop install wixtoolset nsis
  exit /b 1
)
set "MAKENSIS=%TOOL_PATH%"
call :ensure_release_bin
if errorlevel 1 exit /b 1

echo ==^> Building NSIS installer (v%VERSION%)...
"%MAKENSIS%" /DVERSION=%VERSION% "/DPROJECT_ROOT=%CD%" packaging\windows\realraw.nsi
if errorlevel 1 exit /b 1
echo ==^> Done: target\release\%BIN_NAME%-%VERSION%-setup.exe
exit /b 0

:cmd_wix
call :find_tool candle
if not defined TOOL_PATH (
  echo error: 'candle' (WiX Toolset v3) is required but not installed.
  echo install with:
  echo   scoop bucket add extras
  echo   scoop install wixtoolset nsis
  exit /b 1
)
set "CANDLE=%TOOL_PATH%"

call :find_tool light
if not defined TOOL_PATH (
  echo error: 'light' (WiX Toolset v3) is required but not installed.
  echo install with:
  echo   scoop bucket add extras
  echo   scoop install wixtoolset nsis
  exit /b 1
)
set "LIGHT=%TOOL_PATH%"

call :ensure_release_bin
if errorlevel 1 exit /b 1

set "WIXOBJ=target\release\%BIN_NAME%.wixobj"
set "MSI=target\release\%BIN_NAME%-%VERSION%-x64.msi"
echo ==^> Building WiX MSI (v%VERSION%)...
"%CANDLE%" -nologo -arch x64 -dProductVersion=%VERSION% -out "%WIXOBJ%" packaging\windows\realraw.wxs
if errorlevel 1 exit /b 1
"%LIGHT%" -nologo -out "%MSI%" "%WIXOBJ%"
if errorlevel 1 exit /b 1
echo ==^> Done: %MSI%
exit /b 0

:cmd_all
call :cmd_exe
if errorlevel 1 exit /b 1

call :find_tool makensis
if defined TOOL_PATH (
  call :cmd_nsis
  if errorlevel 1 exit /b 1
) else (
  echo ==^> Skipping NSIS (makensis not found)
)

call :find_tool candle
if not defined TOOL_PATH (
  echo ==^> Skipping WiX (candle not found)
  exit /b 0
)
call :find_tool light
if defined TOOL_PATH (
  call :cmd_wix
  if errorlevel 1 exit /b 1
) else (
  echo ==^> Skipping WiX (light not found)
)
exit /b 0
