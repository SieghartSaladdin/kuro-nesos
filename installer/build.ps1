# Builds the release exe and packages it into dist\:
#   KuroNesos-<version>-Setup.exe      Inno Setup installer (per-user, no admin rights)
#   KuroNesos-<version>-portable.zip   kuro-nesos.exe + portable.txt + LICENSE
#   kuro-nesos.exe                     the plain single-file exe
# Needs Inno Setup 6 (ISCC.exe); set $env:ISCC if it is not in a standard location.
param([switch]$NoBuild)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
# native tools write progress to stderr, which "Stop" would treat as a failure
function Native([scriptblock]$run, [string]$what) {
    $ErrorActionPreference = "Continue"
    & $run 2>&1 | ForEach-Object { "$_" }
    if ($LASTEXITCODE) { throw "$what failed" }
}
if (-not $NoBuild) { Native { cargo build --release } "cargo build" }
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
$exe = Join-Path $targetDir "release\kuro-nesos.exe"
if (-not (Test-Path $exe)) { throw "missing $exe" }

$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force $dist | Out-Null
Copy-Item $exe (Join-Path $dist "kuro-nesos.exe") -Force

# portable zip
$stage = Join-Path $dist "portable"
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $exe, (Join-Path $PSScriptRoot "portable.txt") $stage -Force
Copy-Item (Join-Path $root "LICENSE") (Join-Path $stage "LICENSE.txt") -Force
$zip = Join-Path $dist "KuroNesos-$version-portable.zip"
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -Force
Remove-Item $stage -Recurse -Force

# installer
$iscc = @($env:ISCC, "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe", "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe") | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 6 (ISCC.exe) not found; set `$env:ISCC" }
Native { & $iscc /Q "/DAppVersion=$version" "/DSourceExe=$exe" (Join-Path $PSScriptRoot "kuro-nesos.iss") } "ISCC"

Get-ChildItem $dist -File | ForEach-Object { "{0,-40} {1,8:N0} KB  {2}" -f $_.Name, ($_.Length / 1KB), (Get-FileHash $_.FullName -Algorithm SHA256).Hash }
