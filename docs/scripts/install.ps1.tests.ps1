$ErrorActionPreference = "Stop"
$installer = Join-Path $PSScriptRoot "../public/install/install.ps1"
$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) "semifold-install-$PID"
$script:requestedUris = [System.Collections.Generic.List[string]]::new()
$script:releaseRequests = 0

function global:Invoke-WebRequest {
    param(
        [string]$Uri,
        [string]$OutFile
    )

    $script:requestedUris.Add($Uri)
    if ($Uri -match '/releases\?page=') {
        $script:releaseRequests++
        return [pscustomobject]@{
            Content = @'
<a href="/noctisynth/semifold/releases/tag/%40semifold%2Fcli-v9.9.9">CLI</a>
<a href="/noctisynth/semifold/releases/tag/semifold-v9.0.0-rc.1">Prerelease</a>
<a href="/noctisynth/semifold/releases/tag/semifold-v0.3.1">Latest binary</a>
'@
        }
    }
    [System.IO.File]::WriteAllText($OutFile, "semifold-binary")
}

function Assert-Equal {
    param($Actual, $Expected, [string]$Message)
    if ($Actual -ne $Expected) {
        throw "$Message. Expected '$Expected', received '$Actual'"
    }
}

try {
    $latestDirectory = Join-Path $testRoot "latest"
    & $installer -InstallDir $latestDirectory
    Assert-Equal $script:releaseRequests 1 "The default install must query releases"
    Assert-Equal `
        $script:requestedUris[$script:requestedUris.Count - 1] `
        "https://github.com/noctisynth/semifold/releases/download/semifold-v0.3.1/semifold-windows-x86_64.exe" `
        "The default install must select the stable Semifold binary release"
    Assert-Equal `
        ([System.IO.File]::ReadAllText((Join-Path $latestDirectory "semifold.exe"))) `
        "semifold-binary" `
        "The installer must move the downloaded binary into place"

    foreach ($versionCase in @(
        @{ input = "0.3.1"; normalized = "0.3.1" },
        @{ input = "v0.3.1"; normalized = "0.3.1" },
        @{ input = "0.4.0-rc.1"; normalized = "0.4.0-rc.1" }
    )) {
        $explicitDirectory = Join-Path $testRoot $versionCase.input
        $requestsBefore = $script:releaseRequests
        & $installer -Version $versionCase.input -InstallDir $explicitDirectory
        Assert-Equal $script:releaseRequests $requestsBefore "Explicit versions must not query releases"
        Assert-Equal `
            $script:requestedUris[$script:requestedUris.Count - 1] `
            "https://github.com/noctisynth/semifold/releases/download/semifold-v$($versionCase.normalized)/semifold-windows-x86_64.exe" `
            "Explicit versions must use the canonical release tag"
    }
} finally {
    Remove-Item -Path $testRoot -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -Path Function:\Invoke-WebRequest -Force -ErrorAction SilentlyContinue
}
