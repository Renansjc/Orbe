[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$trustRoot = Join-Path $env:LOCALAPPDATA 'mt01-matter-probe\trust'
$paaTarget = Join-Path $trustRoot 'paa'
$cdTarget = Join-Path $trustRoot 'cd'
$temporaryRoot = Join-Path $env:TEMP ('orbe-matter-roots-' + [guid]::NewGuid().ToString('N'))

try {
    New-Item -ItemType Directory -Force -Path $paaTarget, $cdTarget | Out-Null
    git clone --depth 1 --filter=blob:none --sparse https://github.com/project-chip/connectedhomeip.git $temporaryRoot
    git -C $temporaryRoot sparse-checkout set credentials/production/paa-root-certs credentials/production/cd-certs

    $paaFiles = Get-ChildItem -LiteralPath (Join-Path $temporaryRoot 'credentials\production\paa-root-certs') -Recurse -Filter '*.der' -File
    $cdFiles = Get-ChildItem -LiteralPath (Join-Path $temporaryRoot 'credentials\production\cd-certs') -Recurse -Filter '*.der' -File
    if ($paaFiles.Count -eq 0 -or $cdFiles.Count -eq 0) { throw 'O repositório oficial não retornou as raízes PAA/CD esperadas.' }

    Copy-Item -LiteralPath $paaFiles.FullName -Destination $paaTarget -Force
    Copy-Item -LiteralPath $cdFiles.FullName -Destination $cdTarget -Force
    Write-Host "Raízes Matter instaladas: $($paaFiles.Count) PAA e $($cdFiles.Count) CD."
    Write-Host 'Agora execute: cargo run -- commission'
}
finally {
    if (Test-Path -LiteralPath $temporaryRoot) { Remove-Item -LiteralPath $temporaryRoot -Recurse -Force }
}
