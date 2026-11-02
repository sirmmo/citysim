@echo off
rem Downloads 0 A.D. art from the original repository and converts it into one
rem skin per culture (assets\skins\antiquity-<civ>), in Docker Desktop.
rem Needs Docker. The converted files are CC BY-SA 3.0.
setlocal
pushd "%~dp0.."
set "ROOT=%CD%"
popd
where docker >nul 2>&1 || (echo Docker Desktop is needed to download the 0 A.D. art: https://docs.docker.com/get-docker/ & exit /b 1)
echo Preparing the conversion tools...
docker build -q -t citysim-zeroad:latest -f "%ROOT%\docker\zeroad.Dockerfile" "%ROOT%\docker" >nul || exit /b 1
if exist "%ROOT%\assets\skins\generated.toml" del /q "%ROOT%\assets\skins\generated.toml"
for /d %%D in ("%ROOT%\assets\skins\antiquity-*") do rmdir /s /q "%%D"
docker run --rm -v citysim-0ad:/cache -v "%ROOT%":/w -w /w citysim-zeroad:latest python tools/zeroad/bake_0ad.py --cache /cache/repo --config tools/zeroad/antiquity.toml --out assets/skins || exit /b 1
echo 0 A.D. art installed: start citysim and pick an ancient culture in File ^> New city.
