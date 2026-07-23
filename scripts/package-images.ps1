[CmdletBinding()]
param(
    [string]$ImagePrefix = $(if ($env:IMAGE_PREFIX) { $env:IMAGE_PREFIX } else { "helt-blog" }),
    [string]$Tag = $(if ($env:IMAGE_TAG) { $env:IMAGE_TAG } else { "latest" }),
    [string]$OutputDirectory = "release",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
$releaseRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    [IO.Path]::GetFullPath($OutputDirectory)
} else {
    [IO.Path]::GetFullPath((Join-Path $projectRoot $OutputDirectory))
}
$safeTag = $Tag -replace '[^A-Za-z0-9_.-]', '-'
$bundleDirectory = Join-Path $releaseRoot "helt-blog-$safeTag"
$archive = Join-Path $bundleDirectory "images.tar"

Push-Location $projectRoot
try {
    $env:IMAGE_PREFIX = $ImagePrefix
    $env:IMAGE_TAG = $Tag

    if (-not $SkipBuild) {
        & docker compose build backend frontend gateway
        if ($LASTEXITCODE -ne 0) { throw "Docker image build failed." }
    }

    & docker compose pull postgres minio minio-init
    if ($LASTEXITCODE -ne 0) { throw "Dependency image pull failed." }

    New-Item -ItemType Directory -Path $bundleDirectory -Force | Out-Null
    Copy-Item docker-compose.yml, .env.example, DEPLOY.md -Destination $bundleDirectory -Force

    $images = @(
        "${ImagePrefix}-frontend:${Tag}",
        "${ImagePrefix}-backend:${Tag}",
        "${ImagePrefix}-gateway:${Tag}",
        "postgres:16-alpine",
        "minio/minio:latest",
        "minio/mc:latest"
    )
    & docker image save --output $archive @images
    if ($LASTEXITCODE -ne 0) { throw "Docker image export failed." }

    $checksum = Get-FileHash -Algorithm SHA256 -LiteralPath $archive
    "$($checksum.Hash.ToLowerInvariant()) *images.tar" |
        Set-Content -LiteralPath "$archive.sha256" -Encoding ascii

    Write-Host "Offline deployment bundle created at: $bundleDirectory"
} finally {
    Pop-Location
}
