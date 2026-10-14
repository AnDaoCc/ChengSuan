$ErrorActionPreference = 'Stop'
$csRoot = Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $csRoot
if (Test-Path -LiteralPath "$csRoot\.toolchains\cargo\bin\cargo.exe") {
  $env:CARGO_HOME = "$csRoot\.toolchains\cargo"
  $env:RUSTUP_HOME = "$csRoot\.toolchains\rustup"
  $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}
& npm.cmd run release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
New-Item -ItemType Directory -Path "$csRoot\release" -Force | Out-Null
Copy-Item -LiteralPath "$csRoot\src-tauri\target\x86_64-pc-windows-msvc\release\ChengSuan.exe" -Destination "$csRoot\release\ChengSuan.exe" -Force
Get-FileHash -LiteralPath "$csRoot\release\ChengSuan.exe" -Algorithm SHA256
