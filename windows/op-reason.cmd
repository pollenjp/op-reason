@echo off
rem op-reason shim for cmd.exe and other programs: runs op-reason.ps1 in Windows PowerShell 5.1.
rem cmd.exe re-parses the arguments, so an argument without spaces that contains & | < > ^ % can break
rem (see the README). Shells that may run scripts can call op-reason.ps1 directly.
setlocal
set "OPREASON_ENTRY=cmd"
set "OPREASON_ENTRY_NAME=%~n0"
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0op-reason.ps1" %*
exit /b %ERRORLEVEL%
