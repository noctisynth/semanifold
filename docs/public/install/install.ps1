param(
    [Parameter(Position = 0)]
    [string]$Version,

    [string]$InstallDir = (Join-Path $HOME ".local\bin")
)

$ErrorActionPreference = "Stop"

if ($Version -and $Version -notmatch '^[0-9A-Za-z.+-]+$') {
    throw "Invalid version: $Version"
}

$arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "arm64" }
$name = "semifold-windows-$arch.exe"

$bin = $InstallDir
New-Item -ItemType Directory -Force -Path $bin | Out-Null

$releasePath = if ($Version) { "download/semifold-$Version" } else { "latest/download" }
$downloadUrl = "https://github.com/noctisynth/semifold/releases/$releasePath/$name"
$versionDescription = if ($Version) { " version $Version" } else { "" }
$destination = "$bin\semifold.exe"
$temporaryFile = "$destination.tmp.$PID"
Write-Host "[*] Downloading $name$versionDescription ..."
try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $temporaryFile
    Move-Item -Force -Path $temporaryFile -Destination $destination
} finally {
    Remove-Item -Force -ErrorAction SilentlyContinue $temporaryFile
}

Write-Host "[*] Installed semifold to $bin"
Write-Host "[*] Add $bin to your PATH to use it."
