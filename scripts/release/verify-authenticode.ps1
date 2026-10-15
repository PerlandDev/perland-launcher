# Checks the Authenticode signatures of a built installer and of what it installs (signing spec §3.7).
# Runs on the Windows release runner only: it installs the launcher for the current user.
param(
  [Parameter(Mandatory)] [string] $Installer,
  # Release certificate: the signature must be Valid and from SignPath Foundation. Without it (test
  # certificate) a signature and a timestamp must be present, but the root may be untrusted.
  [switch] $RequireTrusted
)
$ErrorActionPreference = 'Stop'
$failures = @()

function Test-File([string] $Path) {
  $sig = Get-AuthenticodeSignature -LiteralPath $Path
  $name = Split-Path $Path -Leaf
  if (-not $sig.SignerCertificate) { return "$name has no signature" }
  if (-not $sig.TimeStamperCertificate) { return "$name has no timestamp" }
  if ($RequireTrusted) {
    if ($sig.Status -ne 'Valid') { return "$name signature is $($sig.Status): $($sig.StatusMessage)" }
    if ($sig.SignerCertificate.Subject -notmatch 'SignPath Foundation') { return "$name is signed by $($sig.SignerCertificate.Subject)" }
  }
  Write-Host "ok  $name  $($sig.Status)  $($sig.SignerCertificate.Subject)"
  return $null
}

$failures += Test-File $Installer

# Tauri's per-user NSIS installer installs to $LOCALAPPDATA\<productName>.
$installDir = Join-Path $env:LOCALAPPDATA 'Perland Launcher'
$p = Start-Process -FilePath $Installer -ArgumentList '/S' -Wait -PassThru
if ($p.ExitCode -ne 0) { $failures += "the installer exited with $($p.ExitCode)" }
$exes = @(Get-ChildItem -LiteralPath $installDir -Filter *.exe -ErrorAction SilentlyContinue)
if ($exes.Count -lt 2) { $failures += "expected the launcher and uninstall.exe in $installDir, found $($exes.Count) .exe files" }
foreach ($exe in $exes) { $failures += Test-File $exe.FullName }

$failures = @($failures | Where-Object { $_ })
foreach ($f in $failures) { Write-Host "::error::$f" }
if ($failures.Count -gt 0) { exit 1 }
