param([Parameter(ValueFromRemainingArguments=$true)][string[]]$CargoArgs)
$ErrorActionPreference = 'Stop'
$csRoot = Split-Path $PSScriptRoot -Parent
$env:CARGO_HOME = Join-Path $csRoot '.toolchains\cargo'
$env:RUSTUP_HOME = Join-Path $csRoot '.toolchains\rustup'
$env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
& (Join-Path $env:CARGO_HOME 'bin\cargo.exe') @CargoArgs
exit $LASTEXITCODE
