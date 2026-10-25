@echo off
rem Installation Windows de Zer0|GH0ST + prérequis Zero-chan.
rem   .\scripts\install.cmd
rem   .\scripts\install.cmd -VoiceOnly
rem   .\scripts\install.cmd -Minimal
rem   .\scripts\install.cmd -SkipBuild
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-windows.ps1" %*
exit /b %ERRORLEVEL%
