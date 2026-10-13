@echo off
rem Lanceur pour contrib-windows.ps1 (contourne la politique d'execution).
rem   .\scripts\contrib.cmd -Message "feat: module 0box"
rem   .\scripts\contrib.cmd -Sync
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0contrib-windows.ps1" %*
