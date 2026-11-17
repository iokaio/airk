REM SPDX-License-Identifier: Apache-2.0
@echo off
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" 10.0.26100.0 -vcvars_ver=14.44.35207 > nul 2>&1
if errorlevel 1 exit /b %errorlevel%
D:\code\Github\airk\artifacts\advanced-split\ort130-x64-owned-r2\tools\cmake\bin\cmake.exe -S D:\code\Github\airk\artifacts\advanced-split\msvc-configure-owner-context-r8\coff-witness -B D:\code\Github\airk\artifacts\advanced-split\msvc-configure-owner-context-r8\coff-witness\empty -G "NMake Makefiles" -DCMAKE_BUILD_TYPE=Release -DCMAKE_POLICY_DEFAULT_CMP0141=NEW -DCMAKE_MSVC_DEBUG_INFORMATION_FORMAT= -DCMAKE_POLICY_DEFAULT_CMP0137=NEW "-DCMAKE_MAKE_PROGRAM=C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\nmake.exe" -DCMAKE_TRY_COMPILE_CONFIGURATION=Release
exit /b %errorlevel%
