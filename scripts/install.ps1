# AlliedID 설치 (Windows PowerShell)
#   - Rust 코어를 빌드해 AlliedID.exe 로 설치
#   - C# 컴패니언 빌드·설치 (Windows 에서는 네트워크 환경 수집에 필요)
#   - 기본 설정·인격체 생성, 기본 지식 팩 빌드
#
# 사용법: powershell -ExecutionPolicy Bypass -File scripts\install.ps1
param(
    [string]$Prefix = (Join-Path $env:LOCALAPPDATA "AlliedID")
)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Bin = Join-Path $Prefix "bin"
New-Item -ItemType Directory -Force -Path $Bin | Out-Null

Write-Host "==> Rust 코어 빌드"
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw "cargo 가 필요합니다: https://rustup.rs" }
cargo build --release --manifest-path (Join-Path $Root "core\Cargo.toml")
Copy-Item (Join-Path $Root "core\target\release\AlliedID.exe") $Bin -Force

Write-Host "==> C# 컴패니언 빌드"
if (Get-Command dotnet -ErrorAction SilentlyContinue) {
    dotnet publish (Join-Path $Root "companion\src\AlliedID.Companion\AlliedID.Companion.csproj") -c Release -o $Bin --nologo -v quiet
} else {
    Write-Warning "dotnet 이 없어 컴패니언을 건너뜁니다. Windows 에서는 네트워크 환경이 접속 시험(probe-only)으로만 파악됩니다."
}

Write-Host "==> 초기 설정"
& (Join-Path $Bin "AlliedID.exe") init

if (Get-Command python -ErrorAction SilentlyContinue) {
    Write-Host "==> 기본 지식 팩 빌드"
    python (Join-Path $Root "tools\knowledge_builder.py") (Join-Path $Root "knowledge\sources\basics") --name allied-id-basics
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (-not ($userPath -split ";" | Where-Object { $_ -eq $Bin })) {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$Bin", "User")
    Write-Host "PATH 에 $Bin 을 추가했습니다. 새 터미널을 여세요."
}
Write-Host "`n완료. 터미널에서 AlliedID 를 입력하세요."
