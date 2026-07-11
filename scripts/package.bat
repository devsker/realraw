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

:find_makensis
set "MAKENSIS="
where makensis >nul 2>&1
if not errorlevel 1 (
  for /f "delims=" %%P in ('where makensis') do (
    set "MAKENSIS=%%P"
    goto :eof
  )
)
if exist "%GITHUB_WORKSPACE%\tools\nsis-3.10\makensis.exe" set "MAKENSIS=%GITHUB_WORKSPACE%\tools\nsis-3.10\makensis.exe" & goto :eof
if exist "%~dp0..\tools\nsis-3.10\makensis.exe" set "MAKENSIS=%~dp0..\tools\nsis-3.10\makensis.exe" & goto :eof
if exist "%~dp0..\tools\nsis\makensis.exe" set "MAKENSIS=%~dp0..\tools\nsis\makensis.exe" & goto :eof
if exist "%USERPROFILE%\scoop\shims\makensis.exe" set "MAKENSIS=%USERPROFILE%\scoop\shims\makensis.exe" & goto :eof
if exist "%USERPROFILE%\scoop\apps\nsis\current\makensis.exe" set "MAKENSIS=%USERPROFILE%\scoop\apps\nsis\current\makensis.exe" & goto :eof
if exist "%ProgramFiles(x86)%\NSIS\makensis.exe" set "MAKENSIS=%ProgramFiles(x86)%\NSIS\makensis.exe" & goto :eof
if exist "%ProgramFiles%\NSIS\makensis.exe" set "MAKENSIS=%ProgramFiles%\NSIS\makensis.exe" & goto :eof
exit /b 1

:find_candle
set "CANDLE="
where candle >nul 2>&1
if not errorlevel 1 (
  for /f "delims=" %%P in ('where candle') do (
    set "CANDLE=%%P"
    goto :find_light_stub
  )
)
if exist "%GITHUB_WORKSPACE%\tools\wix\candle.exe" set "CANDLE=%GITHUB_WORKSPACE%\tools\wix\candle.exe" & goto :find_light_stub
if exist "%~dp0..\tools\wix\candle.exe" set "CANDLE=%~dp0..\tools\wix\candle.exe" & goto :find_light_stub
if exist "%USERPROFILE%\scoop\apps\wixtoolset\current\bin\candle.exe" set "CANDLE=%USERPROFILE%\scoop\apps\wixtoolset\current\bin\candle.exe" & goto :find_light_stub
for /d %%D in ("%ProgramFiles(x86)%\WiX Toolset v*") do (
  if exist "%%D\bin\candle.exe" set "CANDLE=%%D\bin\candle.exe" & goto :find_light_stub
)
for /d %%D in ("%ProgramFiles%\WiX Toolset v*") do (
  if exist "%%D\bin\candle.exe" set "CANDLE=%%D\bin\candle.exe" & goto :find_light_stub
)
exit /b 1

:find_light_stub
if defined CANDLE (
  for %%P in ("!CANDLE!") do if exist "%%~dpPlight.exe" set "LIGHT=%%~dpPlight.exe"
)
goto :eof

:find_light
set "LIGHT="
where light >nul 2>&1
if not errorlevel 1 (
  for /f "delims=" %%P in ('where light') do (
    set "LIGHT=%%P"
    goto :eof
  )
)
if exist "%GITHUB_WORKSPACE%\tools\wix\light.exe" set "LIGHT=%GITHUB_WORKSPACE%\tools\wix\light.exe" & goto :eof
if exist "%~dp0..\tools\wix\light.exe" set "LIGHT=%~dp0..\tools\wix\light.exe" & goto :eof
if exist "%USERPROFILE%\scoop\apps\wixtoolset\current\bin\light.exe" set "LIGHT=%USERPROFILE%\scoop\apps\wixtoolset\current\bin\light.exe" & goto :eof
for /d %%D in ("%ProgramFiles(x86)%\WiX Toolset v*") do (
  if exist "%%D\bin\light.exe" set "LIGHT=%%D\bin\light.exe" & goto :eof
)
for /d %%D in ("%ProgramFiles%\WiX Toolset v*") do (
  if exist "%%D\bin\light.exe" set "LIGHT=%%D\bin\light.exe" & goto :eof
)
if defined CANDLE (
  for %%P in ("!CANDLE!") do if exist "%%~dpPlight.exe" set "LIGHT=%%~dpPlight.exe"
)
exit /b 1

:cmd_nsis
call :find_makensis
if not defined MAKENSIS (
  echo error: 'makensis' is required but not installed.
  echo install with:
  echo   scoop bucket add extras
  echo   scoop install wixtoolset nsis
  exit /b 1
)
call :ensure_release_bin
if errorlevel 1 exit /b 1

echo ==^> Building NSIS installer (v%VERSION%)...
"%MAKENSIS%" /DVERSION=%VERSION% packaging\windows\realraw.nsi
if errorlevel 1 exit /b 1
echo ==^> Done: target\release\%BIN_NAME%-%VERSION%-setup.exe
exit /b 0

:cmd_wix
call :find_candle
call :find_light
if not defined CANDLE (
  echo error: 'candle' (WiX Toolset v3) is required but not installed.
  echo install with:
  echo   scoop bucket add extras
  echo   scoop install wixtoolset nsis
  exit /b 1
)
if not defined LIGHT (
  echo error: 'light' (WiX Toolset v3) is required but not installed.
  echo install with:
  echo   scoop bucket add extras
  echo   scoop install wixtoolset nsis
  exit /b 1
)
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

call :find_makensis
if defined MAKENSIS (
  call :cmd_nsis
  if errorlevel 1 exit /b 1
) else (
  echo ==^> Skipping NSIS (makensis not found)
)

call :find_candle
call :find_light
if defined CANDLE if defined LIGHT (
  call :cmd_wix
  if errorlevel 1 exit /b 1
) else (
  echo ==^> Skipping WiX (candle/light not found)
)
exit /b 0
