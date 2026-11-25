# Install the latest proofback release for the current user.
# Usage: irm https://github.com/Amet13/proofback/releases/latest/download/install.ps1 | iex
$ErrorActionPreference = "Stop"
$repo = "Amet13/proofback"
$arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq "Arm64") { "aarch64" } else { "x86_64" }
$name = "proofback-$arch-pc-windows-msvc"
$base = "https://github.com/$repo/releases/latest/download"
$dest = Join-Path $env:LOCALAPPDATA "proofback"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  Invoke-WebRequest "$base/$name.zip" -OutFile "$tmp/$name.zip"
  Invoke-WebRequest "$base/$name.zip.sha256" -OutFile "$tmp/$name.zip.sha256"
  $expected = (Get-Content "$tmp/$name.zip.sha256").Split(" ")[0].ToLower()
  $actual = (Get-FileHash "$tmp/$name.zip" -Algorithm SHA256).Hash.ToLower()
  if ($expected -ne $actual) { throw "Checksum mismatch for $name.zip" }
  Expand-Archive "$tmp/$name.zip" -DestinationPath $tmp
  if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
  Move-Item "$tmp/$name" $dest
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if (-not ($userPath -split ";" | Where-Object { $_ -eq $dest })) {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$dest", "User")
    Write-Host "Added $dest to your PATH (open a new terminal)."
  }
  Write-Host "proofback installed to $dest"
} finally {
  Remove-Item -Recurse -Force $tmp
}
