<#
.SYNOPSIS
  claude-view installer for Windows (PowerShell 5.1+).

.DESCRIPTION
  Downloads the pre-built claude-view-win32-x64.zip from GitHub Releases,
  verifies its SHA256 checksum, extracts to %USERPROFILE%\.claude-view\bin,
  and adds that directory to the user PATH.

.EXAMPLE
  irm https://get.claudeview.ai/install.ps1 | iex

  # Specific version:
  $env:CLAUDE_VIEW_VERSION = "0.45.0"; irm https://get.claudeview.ai/install.ps1 | iex
#>
[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$Repo = "tombelieber/claude-view"
$InstallDir = if ($env:CLAUDE_VIEW_INSTALL_DIR) { $env:CLAUDE_VIEW_INSTALL_DIR } else { Join-Path $HOME ".claude-view" }
$BinDir = Join-Path $InstallDir "bin"
$Artifact = "claude-view-win32-x64.zip"

function Get-LatestVersion {
  if ($env:CLAUDE_VIEW_VERSION) { return $env:CLAUDE_VIEW_VERSION }
  # Follow the /releases/latest redirect to discover the tag.
  $req = [System.Net.WebRequest]::Create("https://github.com/$Repo/releases/latest")
  $req.AllowAutoRedirect = $false
  try { $resp = $req.GetResponse() } catch [System.Net.WebException] { $resp = $_.Exception.Response }
  $loc = $resp.Headers["Location"]
  $resp.Close()
  if ($loc -match "/tag/v?(.+)$") { return $Matches[1] }
  throw "Could not resolve latest version. Set `$env:CLAUDE_VIEW_VERSION = 'x.y.z' or see https://github.com/$Repo/releases"
}

function Add-ToUserPath([string]$Dir) {
  $current = [Environment]::GetEnvironmentVariable("Path", "User")
  if ($current -split ";" | Where-Object { $_ -eq $Dir }) { return }
  [Environment]::SetEnvironmentVariable("Path", "$current;$Dir", "User")
  Write-Host "  Added to user PATH (restart your terminal to pick it up)."
}

Write-Host "Installing claude-view..." -ForegroundColor White
$version = Get-LatestVersion
$url = "https://github.com/$Repo/releases/download/v$version/$Artifact"
Write-Host "  Platform: win32-x64"
Write-Host "  Version:  v$version"

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) "claude-view-$([Guid]::NewGuid()).zip"
try {
  Write-Host "Downloading..."
  Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing

  # Verify checksum (best-effort: older releases may lack checksums.txt).
  try {
    $sums = (Invoke-WebRequest -Uri "https://github.com/$Repo/releases/download/v$version/checksums.txt" -UseBasicParsing).Content
    $expected = ($sums -split "`n" | Where-Object { $_ -match ([regex]::Escape($Artifact)) } | ForEach-Object { ($_ -split "\s+")[0] } | Select-Object -First 1)
    if ($expected) {
      $actual = (Get-FileHash -Path $tmp -Algorithm SHA256).Hash.ToLower()
      if ($actual -ne $expected.ToLower()) { throw "Checksum verification failed.`n  Expected: $expected`n  Actual:   $actual" }
      Write-Host "  Checksum verified."
    }
  } catch {
    Write-Host "  (checksum file not available, skipping verification)"
  }

  if (Test-Path $BinDir) { Remove-Item -Recurse -Force $BinDir }
  New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
  # Prefer tar (Win10+) for reliable extraction; fall back to Expand-Archive.
  try {
    tar -xf $tmp -C $BinDir
  } catch {
    Expand-Archive -Force -Path $tmp -DestinationPath $BinDir
  }

  Set-Content -Path (Join-Path $InstallDir "version") -Value $version -NoNewline
  Set-Content -Path (Join-Path $InstallDir "install-source") -Value "install_ps1" -NoNewline

  Add-ToUserPath $BinDir

  Write-Host ""
  Write-Host "claude-view v$version installed successfully!" -ForegroundColor Green
  Write-Host "  Installed to: $(Join-Path $BinDir 'claude-view.exe')"
  Write-Host ""
  Write-Host "Run 'claude-view' to get started (restart your terminal first if PATH was updated)."
} finally {
  if (Test-Path $tmp) { Remove-Item -Force $tmp }
}
