# Removes Windows Image Viewer's registration, Start menu shortcut and installed copy.
$ErrorActionPreference = 'Stop'
$dest = Join-Path $env:LOCALAPPDATA 'Programs\Windows Image Viewer'
$exe = Join-Path $dest 'imageviewer.exe'
if (Test-Path $exe) {
    Start-Process $exe -ArgumentList '--unregister' -Wait
}
Remove-Item (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Windows Image Viewer.lnk') -ErrorAction SilentlyContinue
Remove-Item $dest -Recurse -Force -ErrorAction SilentlyContinue
Write-Host 'Uninstalled. Windows falls back to its previous default for any types you had assigned.'
