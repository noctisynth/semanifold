$ErrorActionPreference = "Stop"

$arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "arm64" }
$name = "semifold-windows-$arch.exe"

$bin = "$HOME\.local\bin"
New-Item -ItemType Directory -Force -Path $bin | Out-Null

$downloadUrl = "https://github.com/noctisynth/semifold/releases/latest/download/$name"
Write-Host "[*] Downloading $name ..."
Invoke-WebRequest -Uri $downloadUrl -OutFile "$bin\semifold.exe"

Write-Host "[*] Installed semifold to $bin"
Write-Host "[*] Add $bin to your PATH to use it."
