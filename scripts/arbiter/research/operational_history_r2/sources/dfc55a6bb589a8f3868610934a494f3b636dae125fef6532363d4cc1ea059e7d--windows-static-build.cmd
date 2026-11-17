REM SPDX-License-Identifier: Apache-2.0
@echo off
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" 10.0.26100.0 > "D:\code\Github\airk\artifacts\local\one-airk-1.8.14\build-toolchain.log" 2>&1
if errorlevel 1 exit /b %errorlevel%
"C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe" --build D:\code\Github\airk\artifacts\local\one-airk-1.8.14\b\Release --config Release --parallel 2
exit /b %errorlevel%
