param(
    [Parameter(Position = 0)]
    [string]$Version,

    [string]$InstallDir = (Join-Path $HOME ".local\bin")
)

$ErrorActionPreference = "Stop"

function Normalize-Version {
    param([string]$Value)

    $normalized = $Value -replace '^v', ''
    if ($normalized -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
        throw "Invalid version: $Value"
    }
    return $normalized
}

if ($Version) {
    $resolvedVersion = Normalize-Version $Version
} else {
    Write-Host "[*] Resolving the latest stable Semifold release ..."
    $page = 1
    $resolvedVersion = $null
    do {
        try {
            $response = Invoke-WebRequest `
                -Uri "https://github.com/noctisynth/semifold/releases?page=$page"
        } catch {
            throw "Failed to query Semifold releases: $($_.Exception.Message)"
        }

        $releaseMatch = [regex]::Match(
            $response.Content,
            '/noctisynth/semifold/releases/tag/semifold-v(?<version>[0-9]+\.[0-9]+\.[0-9]+)"'
        )
        if ($releaseMatch.Success) {
            $resolvedVersion = $releaseMatch.Groups['version'].Value
            break
        }

        $hasNextPage = $response.Content -match 'rel="next"'
        $page++
    } while ($hasNextPage)

    if (-not $resolvedVersion) {
        throw "No stable Semifold binary release was found"
    }
}

$arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "arm64" }
$name = "semifold-windows-$arch.exe"

$bin = $InstallDir
New-Item -ItemType Directory -Force -Path $bin | Out-Null

$releasePath = "download/semifold-v$resolvedVersion"
$downloadUrl = "https://github.com/noctisynth/semifold/releases/$releasePath/$name"
$destination = "$bin\semifold.exe"
$temporaryFile = "$destination.tmp.$PID"
Write-Host "[*] Downloading $name version $resolvedVersion ..."
try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $temporaryFile
    Move-Item -Force -Path $temporaryFile -Destination $destination
} finally {
    Remove-Item -Force -ErrorAction SilentlyContinue $temporaryFile
}

Write-Host "[*] Installed semifold to $bin"
Write-Host "[*] Add $bin to your PATH to use it."
