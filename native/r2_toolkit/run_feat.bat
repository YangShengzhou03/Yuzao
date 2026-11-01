@echo off
setlocal
rem Run the C API feature probe against an already running headless Creo.
rem
rem   1) start_creo.bat            (headless Creo, -i:rpc_input)
rem   2) run_feat.bat [partname]
rem
rem Set CREO_ROOT to override the load point.

if "%CREO_ROOT%"=="" set "CREO_ROOT=D:\Program Files\PTC\Creo 12.4.0.0"
set "TEXT=%CREO_ROOT%\Common Files\text"
set "PATH=%CREO_ROOT%\Common Files\x86e_win64\obj;%CREO_ROOT%\Parametric\x86e_win64\obj;%CREO_ROOT%\Parametric\bin;%CREO_ROOT%\Common Files\x86e_win64\lib;%PATH%"
if "%PTC_D_LICENSE_FILE%"=="" set "PTC_D_LICENSE_FILE=C:\Lincense\PTC_D_SSQ.dat"
set "PRO_COMM_MSG_EXE=%CREO_ROOT%\Common Files\x86e_win64\obj\pro_comm_msg.exe"

cd /d "%~dp0"
set "PROBE=%~dp0probe_feat.exe"

if "%~1"=="" goto default
"%PROBE%" "%~1" "%TEXT%"
goto done

:default
"%PROBE%" aicad_feat "%TEXT%"

:done
echo EXIT=%ERRORLEVEL%
