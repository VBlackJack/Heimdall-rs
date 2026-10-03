@echo off
rem Copyright 2026 Julien Bombled
rem
rem Licensed under the Apache License, Version 2.0 (the "License");
rem you may not use this file except in compliance with the License.
rem You may obtain a copy of the License at
rem
rem     http://www.apache.org/licenses/LICENSE-2.0
rem
rem Unless required by applicable law or agreed to in writing, software
rem distributed under the License is distributed on an "AS IS" BASIS,
rem WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
rem See the License for the specific language governing permissions and
rem limitations under the License.
rem
rem Builds Heimdall-rs in release and runs it as a user would: no console, the speed of
rem the shipped build. This window waits for it to close, then shows the end of its log.
rem
rem Usage: run-release.bat [level]
rem   level  error, warn, info (default), debug or trace.

setlocal
cd /d "%~dp0"

set "LEVEL=%~1"
if "%LEVEL%"=="" set "LEVEL=info"
set "HEIMDALL_LOG=%LEVEL%"
set "LOG_FILE=%LOCALAPPDATA%\Heimdall-rs\data\logs\heimdall.log"

echo Building Heimdall-rs in release...
cargo build --release --package heimdall-ui
if errorlevel 1 (
    echo The build failed.
    exit /b 1
)

echo Heimdall-rs, release build, log level %LEVEL%: close it to see its log here.
start "" /wait "target\release\heimdall-rs.exe"
set "RESULT=%ERRORLEVEL%"

echo.
echo Exit code: %RESULT%
call :show_log
exit /b %RESULT%

:show_log
if not exist "%LOG_FILE%" (
    echo No log file at %LOG_FILE%
    exit /b 0
)
echo Log: %LOG_FILE%
echo Last lines:
powershell -NoProfile -Command "Get-Content -LiteralPath $env:LOG_FILE -Tail 40"
exit /b 0
