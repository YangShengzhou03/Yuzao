@echo off
setlocal
rem Start a Creo session in the documented async/non-interactive form, so that
rem `run.bat connect` can attach to it. Leaves Creo running; stop.bat kills it.
rem
rem Usage: start_creo.bat [work_dir]

if "%CREO_ROOT%"=="" set "CREO_ROOT=D:\Program Files\PTC\Creo 12.4.0.0"
if "%~1"=="" (set "WORK=%~dp0work") else (set "WORK=%~1")
if not exist "%WORK%" mkdir "%WORK%"

set "PATH=%CREO_ROOT%\Common Files\x86e_win64\obj;%CREO_ROOT%\Parametric\x86e_win64\obj;%CREO_ROOT%\Parametric\bin;%CREO_ROOT%\Common Files\x86e_win64\lib;%PATH%"
if "%PTC_D_LICENSE_FILE%"=="" set "PTC_D_LICENSE_FILE=C:\Lincense\PTC_D_SSQ.dat"
set "PRO_COMM_MSG_EXE=%CREO_ROOT%\Common Files\x86e_win64\obj\pro_comm_msg.exe"

cd /d "%WORK%"
start "" "%CREO_ROOT%\Parametric\bin\parametric.exe" -g:no_graphics -i:rpc_input
echo [start_creo] launched, work dir = %WORK%
