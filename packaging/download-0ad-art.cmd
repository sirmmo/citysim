@echo off
rem Downloads the 0 A.D. buildings and trees for the ancient cultures and converts
rem them (about 115 MB, from the official 0 A.D. repository). No Docker needed.
"%~dp0opencity-cli.exe" bake-0ad %*
pause
