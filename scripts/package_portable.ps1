# Builds the portable zip from the release exe. Run after `pnpm tauri build`:
#   powershell -ExecutionPolicy Bypass -File scripts\package_portable.ps1
# Output: src-tauri\target\release\bundle\portable\Lens_<version>_x64-portable.zip
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$version = (Get-Content -Raw -Encoding UTF8 (Join-Path $root 'src-tauri\tauri.conf.json') | ConvertFrom-Json).version
$exe = Join-Path $root 'src-tauri\target\release\lens.exe'
if (-not (Test-Path $exe)) { throw "Missing $exe - run pnpm tauri build first" }

$outDir = Join-Path $root 'src-tauri\target\release\bundle\portable'
$stage = Join-Path $outDir 'stage'
$app = Join-Path $stage 'Lens'
if (Test-Path $outDir) { Remove-Item -Recurse -Force $outDir }
New-Item -ItemType Directory -Force $app | Out-Null

Copy-Item $exe (Join-Path $app 'Lens.exe')
Copy-Item (Join-Path $root 'src-tauri\windows\portable.txt') (Join-Path $app 'portable.txt')

$zip = Join-Path $outDir "Lens_${version}_x64-portable.zip"
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($stage, $zip, [IO.Compression.CompressionLevel]::Optimal, $false)
Remove-Item -Recurse -Force $stage
Write-Output $zip
