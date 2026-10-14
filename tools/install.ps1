# Builds Windows Image Viewer from source, installs it for the current user and registers it
# with Windows. No admin rights needed. Undo with tools\uninstall.ps1.
# (With a downloaded exe you don't need this: use Settings > "Set up…" in the app.)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$dest = Join-Path $env:LOCALAPPDATA 'Programs\Windows Image Viewer'
$exe = Join-Path $dest 'imageviewer.exe'

cargo build --release --manifest-path (Join-Path $root 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Build failed' }

New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item (Join-Path $root 'target\release\imageviewer.exe') $exe -Force

$p = Start-Process $exe -ArgumentList '--register' -Wait -PassThru
if ($p.ExitCode -ne 0) { throw 'Registration failed' }

$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut((Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Windows Image Viewer.lnk'))
$link.TargetPath = $exe
$link.Save()

Write-Host "Installed to $exe"
Write-Host 'Opening Default apps: choose Windows Image Viewer for each image type you want.'
Start-Process 'ms-settings:defaultapps?registeredAppUser=Windows%20Image%20Viewer'
