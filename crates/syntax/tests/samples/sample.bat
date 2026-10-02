@echo off
REM Build everything
set TARGET=release
if "%1"=="debug" set TARGET=debug
call build.cmd %TARGET%
goto :eof
