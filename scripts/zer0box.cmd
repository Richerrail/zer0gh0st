@echo off
rem Alias du débat 0box : `zer0box <sujet> [--perspectives code,reasoning,main]`
rem Équivaut exactement à `zer0 0box <sujet>` — sert directement le binaire local.
"%~dp0..\target\release\zer0.exe" 0box %*
