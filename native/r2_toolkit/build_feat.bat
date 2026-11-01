@echo off
setlocal
rem Build the Creo TOOLKIT C API feature probe (P1) and the FeatureSpec
rem builder feat_build.exe (P1.2/P1.3, see ROADMAP).
rem Override the Creo load point with: set CREO_ROOT=...  before running.

if "%CREO_ROOT%"=="" set "CREO_ROOT=D:/Program Files\PTC\Creo 12.4.0.0"
set "PTK=%CREO_ROOT%\Common Files\protoolkit"

call "C:/Program Files/Microsoft Visual Studio/2022/Community/VC/Auxiliary/Build/vcvars64.bat" >nul 2>&1
if errorlevel 1 (
  echo [build] Visual Studio 2022 x64 toolchain not found
  exit /b 1
)

cd /d "%~dp0"

set "LIBS=%PTK%\x86e_win64\obj\pt_asynchronous.lib %PTK%\x86e_win64\obj\protoolkit_NU.lib %PTK%\x86e_win64\obj\ucore.lib %PTK%\x86e_win64\obj\udata.lib kernel32.lib user32.lib wsock32.lib advapi32.lib mpr.lib winspool.lib netapi32.lib psapi.lib gdi32.lib shell32.lib comdlg32.lib ole32.lib ws2_32.lib"

cl /nologo /EHsc /GS /fp:precise /DPRO_USE_VAR_ARGS /DPRO_MACHINE=36 /DPRO_OS=4 /I"%PTK%\includes" probe_feat.c /link /subsystem:console /machine:amd64 /ignore:4049 /ignore:4219 /ignore:4217 /out:probe_feat.exe %LIBS%
echo PROBE_EXIT=%ERRORLEVEL%

cl /nologo /utf-8 /W4 /D_CRT_SECURE_NO_WARNINGS /EHsc /GS /fp:precise /DPRO_USE_VAR_ARGS /DPRO_MACHINE=36 /DPRO_OS=4 /I"%PTK%\includes" feat_build.c /link /subsystem:console /machine:amd64 /ignore:4049 /ignore:4219 /ignore:4217 /out:feat_build.exe %LIBS%
echo BUILD_EXIT=%ERRORLEVEL%
