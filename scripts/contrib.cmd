@echo off

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0contrib-windows.ps1" %*
