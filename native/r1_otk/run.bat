@echo off
setlocal
rem Run the OTK async probe.
rem
rem   run.bat ni         -> spawn: Start("parametric -g:no_graphics -i:rpc_input", text)
rem   run.bat bare       -> spawn: Start("parametric", text)
rem   run.bat exe        -> spawn: Start("<loadpoint>\parametric.exe", text)
rem   run.bat psf        -> spawn: Start("<exe> <parametric.psf>", text)
rem   run.bat connect    -> attach to a Creo already started with -i:rpc_input
rem
rem Set CREO_ROOT to override the load point.

if "%CREO_ROOT%"=="" set "CREO_ROOT=D:\Program Files\PTC\Creo 12.4.0.0"
set "TEXT=%CREO_ROOT%\Common Files\text"
set "PATH=%CREO_ROOT%\Common Files\x86e_win64\obj;%CREO_ROOT%\Parametric\x86e_win64\obj;%CREO_ROOT%\Parametric\bin;%CREO_ROOT%\Common Files\x86e_win64\lib;%PATH%"
if "%PTC_D_LICENSE_FILE%"=="" set "PTC_D_LICENSE_FILE=C:\Lincense\PTC_D_SSQ.dat"
rem Required by asynchronous mode (Creo TOOLKIT User's Guide, "Setting up an
rem Asynchronous Creo Toolkit Application").
set "PRO_COMM_MSG_EXE=%CREO_ROOT%\Common Files\x86e_win64\obj\pro_comm_msg.exe"

cd /d "%~dp0"
set "PROBE=%~dp0probe_otk.exe"

if /I "%~1"=="connect" goto connect
if /I "%~1"=="exe" goto exe
if /I "%~1"=="psf" goto psf
if /I "%~1"=="bare" goto bare
goto ni

:connect
"%PROBE%" connect "%TEXT%"
goto done

:ni
"%PROBE%" start "parametric -g:no_graphics -i:rpc_input" "%TEXT%"
goto done

:bare
"%PROBE%" start "parametric" "%TEXT%"
goto done

:exe
"%PROBE%" start "\"%CREO_ROOT%\Parametric\bin\parametric.exe\"" "%TEXT%"
goto done

:psf
"%PROBE%" start "\"%CREO_ROOT%\Parametric\bin\parametric.exe\" \"%CREO_ROOT%\Parametric\bin\parametric.psf\"" "%TEXT%"
goto done

:done
echo EXIT=%ERRORLEVEL%
