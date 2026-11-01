@echo off
setlocal
rem Kill Creo processes left behind by the probe.
taskkill /F /IM xtop.exe 2>nul
taskkill /F /IM parametric.exe 2>nul
taskkill /F /IM pro_comm_msg.exe 2>nul
taskkill /F /IM nmsd.exe 2>nul
echo [stop_creo] done
