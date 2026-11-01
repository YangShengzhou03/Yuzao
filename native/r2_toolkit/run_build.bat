@echo off
setlocal
rem Run the FeatureSpec-driven feature builder (P1.2/P1.3).
rem
rem   run_build.bat <spec.json>   -> parse+validate+build against headless Creo
rem   (Creo must already be started via start_creo.bat; connect errors are
rem    reported as FS209 until the C API rpc_input session issue is resolved.)
rem
rem Set CREO_ROOT to override the load point.

if "%CREO_ROOT%"=="" set "CREO_ROOT=D:/Program Files\PTC\Creo 12.4.0.0"
set "TEXT=%CREO_ROOT%\Common Files\text"
set "PATH=%CREO_ROOT%\Common Files\x86e_win64\obj;%CREO_ROOT%\Parametric\x86e_win64\obj;%CREO_ROOT%\Parametric\bin;%CREO_ROOT%\Common Files\x86e_win64\lib;%PATH%"
if "%PTC_D_LICENSE_FILE%"=="" set "PTC_D_LICENSE_FILE=C:/Lincense/PTC_D_SSQ.dat"
set "PRO_COMM_MSG_EXE=%CREO_ROOT%\Common Files\x86e_win64\obj\pro_comm_msg.exe"

cd /d "%~dp0"

if "%~1"=="" (
  echo usage: run_build.bat ^<spec.json^>
  exit /b 2
)
"%~dp0feat_build.exe" "%~1" "%TEXT%"
echo EXIT=%ERRORLEVEL%
