REM SPDX-License-Identifier: Apache-2.0
@echo off
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" 10.0.26100.0 -vcvars_ver=14.44.35207 > nul 2>&1
if errorlevel 1 exit /b %errorlevel%
D:\code\Github\airk\artifacts\advanced-split\ort130-x64-owned-r2\tools\cmake\bin\cmake.exe --build D:\code\Github\airk\artifacts\advanced-split\msvc-configure-owner-context-r8\coff-witness\empty --verbose
exit /b %errorlevel%
