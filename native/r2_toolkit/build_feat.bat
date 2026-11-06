@echo off
setlocal
rem Build the Creo TOOLKIT C API feature probe (P1) and the FeatureSpec
rem builder feat_build.exe (P1.2/P1.3, see ROADMAP).
rem Override the Creo load point with: set CREO_ROOT=...  before running.

if "%CREO_ROOT%"=="" set "CREO_ROOT=D:/Program Files/PTC/Creo 12.4.0.0"
set "PTK=%CREO_ROOT%\Common Files\protoolkit"
set "OTK=%CREO_ROOT%\Common Files\otk\otk_cpp"

call "C:/Program Files/Microsoft Visual Studio/2022/Community/VC/Auxiliary/Build/vcvars64.bat" >nul 2>&1
if errorlevel 1 (
  echo [build] Visual Studio 2022 x64 toolchain not found
  exit /b 1
)

cd /d "%~dp0"

rem 路径含空格（Program Files），必须逐个加引号，否则 cl 会在空格处截断
set "SYS_LIBS=kernel32.lib user32.lib wsock32.lib advapi32.lib mpr.lib winspool.lib netapi32.lib psapi.lib gdi32.lib shell32.lib comdlg32.lib ole32.lib ws2_32.lib"
set "PTK_LIBS="%PTK%\x86e_win64\obj\pt_asynchronous.lib" "%PTK%\x86e_win64\obj\protoolkit_NU.lib" "%PTK%\x86e_win64\obj\ucore.lib" "%PTK%\x86e_win64\obj\udata.lib""
set "OTK_LIBS="%OTK%\x86e_win64\obj\otk_cpp_async.lib" "%OTK%\x86e_win64\obj\otk_222.lib""

cl /nologo /EHsc /GS /fp:precise /DPRO_USE_VAR_ARGS /DPRO_MACHINE=36 /DPRO_OS=4 /I"%PTK%\includes" probe_feat.c /link /subsystem:console /machine:amd64 /ignore:4049 /ignore:4219 /ignore:4217 /out:probe_feat.exe %PTK_LIBS% %SYS_LIBS%
echo PROBE_EXIT=%ERRORLEVEL%

rem feat_build 需要 OTK 桥：混合模式 = OTK Connect 建会话 + C Toolkit 建特征
cl /nologo /utf-8 /W4 /D_CRT_SECURE_NO_WARNINGS /EHsc /GS /fp:precise /DPRO_USE_VAR_ARGS /DPRO_MACHINE=36 /DPRO_OS=4 /I"%PTK%\includes" /I"%OTK%\include" feat_build.c otk_bridge.cpp /link /subsystem:console /machine:amd64 /ignore:4049 /ignore:4219 /ignore:4217 /out:feat_build.exe %PTK_LIBS% %OTK_LIBS% %SYS_LIBS%
echo BUILD_EXIT=%ERRORLEVEL%
