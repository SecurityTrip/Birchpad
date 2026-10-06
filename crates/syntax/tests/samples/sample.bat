@echo off
REM Build everything
set TARGET=release
if "%1"=="debug" set TARGET=debug
call build.cmd %TARGET%
goto :eof

:build
set /a COUNT=3
if %COUNT% GTR 2 echo many
exit /b 0
