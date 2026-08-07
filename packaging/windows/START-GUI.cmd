@echo off
setlocal
cd /d "%~dp0"

if not exist "TaiwanGovernmentNews-GUI.exe" (
  echo Cannot find TaiwanGovernmentNews-GUI.exe in this folder.
  pause
  exit /b 1
)

start "" "%~dp0TaiwanGovernmentNews-GUI.exe"
