@echo off
rem AlliedID — Windows 에서 한 번에 실행. 필요하면 빌드한 뒤 시작한다.
setlocal
set "ROOT=%~dp0"
set "ROOT=%ROOT:~0,-1%"
set "OUT=%ROOT%\.build"

if not exist "%OUT%\AlliedID.exe" (
  where cargo >nul 2>nul || (echo cargo 가 필요해요: https://rustup.rs & exit /b 1)
  where dotnet >nul 2>nul || (echo .NET 10 SDK 가 필요해요: https://dotnet.microsoft.com/download & exit /b 1)
  echo 본체를 빌드하는 중… (Rust 엔진 + C# 호스트)
  dotnet build "%ROOT%\host\AlliedID\AlliedID.csproj" -c Release -o "%OUT%" --nologo -v quiet -clp:ErrorsOnly || exit /b 1
)

set "ALLIEDID_ROOT=%ROOT%"
"%OUT%\AlliedID.exe" %*
