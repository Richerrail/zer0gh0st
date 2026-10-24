<#
.SYNOPSIS
  Installe Zer0│GH0ST et les prérequis Zero-chan sous Windows.

.DESCRIPTION
  Par défaut : Rust, Git, Visual Studio Build Tools (C++), FFmpeg, SoX_ng,
  Python 3.12 + faster-whisper, puis compilation release de Zer0.

  -VoiceOnly : installe uniquement la chaîne vocale Zero-chan.
  -Minimal   : installe seulement la chaîne de compilation (sans voix/média).
  -SkipBuild : n'effectue pas la compilation Cargo.

.EXAMPLE
  .\scripts\install.cmd

.EXAMPLE
  .\scripts\install.cmd -VoiceOnly

.EXAMPLE
  .\scripts\install-windows.ps1 -SkipBuild
#>
[CmdletBinding()]
param(
  [switch]$VoiceOnly,
  [switch]$Minimal,
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
try {
  [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
  $OutputEncoding = [System.Text.UTF8Encoding]::new($false)
} catch { }
$ProjectRoot = Split-Path -Parent $PSScriptRoot

function Info($message) { Write-Host "-> $message" -ForegroundColor Cyan }
function Ok($message)   { Write-Host "OK $message" -ForegroundColor Green }
function Warn($message) { Write-Host "!  $message" -ForegroundColor Yellow }
function Die($message)  { Write-Host "X  $message" -ForegroundColor Red; exit 1 }

function Refresh-Path {
  $machine = [Environment]::GetEnvironmentVariable("Path", "Machine")
  $user = [Environment]::GetEnvironmentVariable("Path", "User")
  $env:Path = (($machine, $user) -join ";")
  if ($env:Path -and -not $env:Path.EndsWith(";")) { $env:Path += ";" }
  $cargoBin = Join-Path $HOME ".cargo\bin"
  if (Test-Path $cargoBin) { $env:Path = "$cargoBin;$env:Path" }
}

function Has-Command([string]$name) {
  return [bool](Get-Command $name -ErrorAction SilentlyContinue)
}

# Windows PowerShell transforme la sortie stderr d'un programme natif en
# erreur PowerShell. On la laisse passer sans interrompre le script, puis on
# contrôle explicitement le code de sortie.
function Invoke-NativeCommand(
  [string]$FilePath,
  [string[]]$ArgumentList
) {
  $previous = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    & $FilePath @ArgumentList | Out-Host
    return $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $previous
  }
}

function Install-WingetPackage(
  [string]$Id,
  [string]$Override = ""
) {
  if (-not (Has-Command "winget.exe")) {
    Die "WinGet est introuvable. Installe « App Installer » depuis le Microsoft Store."
  }
  Info "Installation WinGet : $Id"
  $wingetArgs = @(
    "install", "--id", $Id, "--exact",
    "--accept-package-agreements", "--accept-source-agreements",
    "--silent", "--disable-interactivity"
  )
  if ($Override) { $wingetArgs += @("--override", $Override) }
  $code = Invoke-NativeCommand "winget.exe" $wingetArgs
  if ($code -ne 0) { Die "Échec de l'installation WinGet : $Id" }
  Refresh-Path
}

function Test-VcTools {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
  if (-not (Test-Path $vswhere)) { return $false }
  $installation = & $vswhere -latest -products * `
    -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    -property installationPath 2>$null
  return [bool]$installation
}

function Find-Python312 {
  $commands = @("python3.12.exe", "python.exe")
  foreach ($name in $commands) {
    $command = Get-Command $name -ErrorAction SilentlyContinue
    if (-not $command) { continue }
    try {
      $version = & $command.Source -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')" 2>$null
      if ($LASTEXITCODE -eq 0 -and $version.Trim() -eq "3.12") { return $command.Source }
    } catch { }
  }

  $launcher = Get-Command "py.exe" -ErrorAction SilentlyContinue
  if ($launcher) {
    try {
      $candidate = & $launcher.Source -3.12 -c "import sys; print(sys.executable)" 2>$null
      if ($LASTEXITCODE -eq 0 -and $candidate) { return $candidate.Trim() }
    } catch { }
  }

  $candidates = @(
    (Join-Path $env:LOCALAPPDATA "Programs\Python\Python312\python.exe"),
    (Join-Path $env:ProgramFiles "Python312\python.exe")
  )
  foreach ($candidate in $candidates) {
    if (Test-Path $candidate) { return $candidate }
  }
  return $null
}

function Install-CoreTools {
  if (-not (Has-Command "git.exe")) {
    Install-WingetPackage "Git.Git"
  } else { Ok "Git déjà installé" }

  Refresh-Path
  if (-not (Has-Command "cargo.exe")) {
    Install-WingetPackage "Rustlang.Rustup"
    Refresh-Path
    if (Has-Command "rustup.exe") {
      Info "Installation de la toolchain Rust stable…"
      $code = Invoke-NativeCommand "rustup.exe" @("toolchain", "install", "stable")
      if ($code -ne 0) { Die "Échec de l'installation de la toolchain Rust stable." }
      $code = Invoke-NativeCommand "rustup.exe" @("default", "stable")
      if ($code -ne 0) { Die "Échec de la sélection de la toolchain Rust stable." }
      Refresh-Path
    }
  } else { Ok "Rust/Cargo déjà installé" }

  if (-not (Test-VcTools)) {
    $override = "--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
    Install-WingetPackage "Microsoft.VisualStudio.2022.BuildTools" $override
    Refresh-Path
    if (-not (Test-VcTools)) {
      Die "Visual Studio Build Tools n'expose toujours pas les outils C++. Relance le script après installation."
    }
  } else { Ok "Visual Studio Build Tools (C++) déjà installé" }
}

function Install-VoiceTools {
  Refresh-Path

  if (-not (Has-Command "ffmpeg.exe")) {
    Install-WingetPackage "Gyan.FFmpeg"
  } else { Ok "FFmpeg déjà installé" }

  if (-not (Has-Command "sox_ng.exe")) {
    Install-WingetPackage "sox_ng.sox_ng"
  } else { Ok "SoX_ng déjà installé" }

  $python = Find-Python312
  if (-not $python) {
    Install-WingetPackage "Python.Python.3.12"
    Refresh-Path
    $python = Find-Python312
  }
  if (-not $python) { Die "Python 3.12 introuvable après l'installation WinGet." }
  Ok "Python 3.12 : $python"

  $venv = Join-Path $HOME ".venvs\zer0-whisper"
  $venvPython = Join-Path $venv "Scripts\python.exe"
  if (-not (Test-Path $venvPython)) {
    Info "Création du venv STT : $venv"
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $venv) | Out-Null
    $code = Invoke-NativeCommand $python @("-m", "venv", $venv)
    if ($code -ne 0) { Die "Création du venv Python impossible." }
  } else { Ok "venv STT déjà présent : $venv" }

  # Certains Python (uv, distributions embarquées) créent un venv sans pip.
  $pipReady = $false
  try {
    & $venvPython -m pip --version *> $null
    $pipReady = ($LASTEXITCODE -eq 0)
  } catch { $pipReady = $false }
  if (-not $pipReady) {
    Info "Initialisation de pip dans le venv…"
    $code = Invoke-NativeCommand $venvPython @("-m", "ensurepip", "--upgrade")
    if ($code -ne 0) { Die "Impossible d'initialiser pip dans le venv Python." }
  }

  Info "Installation de faster-whisper + PyAV compatible…"
  $code = Invoke-NativeCommand $venvPython @("-m", "pip", "install", "--upgrade", "pip")
  if ($code -ne 0) { Die "Mise à jour de pip impossible." }
  # av 19 a retiré `metadata_errors`, encore utilisé par faster-whisper 1.2.1.
  $code = Invoke-NativeCommand $venvPython @("-m", "pip", "install", "--upgrade", "faster-whisper", "av<16")
  if ($code -ne 0) { Die "Installation de faster-whisper impossible." }

  $code = Invoke-NativeCommand $venvPython @("-c", "import faster_whisper, av; print(f'   faster-whisper {faster_whisper.__version__} - PyAV {av.__version__}')")
  if ($code -ne 0) { Die "Le venv STT est installé mais ses imports échouent." }

  [Environment]::SetEnvironmentVariable("ZER0_PYTHON", $venvPython, "User")
  $env:ZER0_PYTHON = $venvPython
  Ok "ZER0_PYTHON configuré : $venvPython"
  Ok "Zer0 détecte aussi ce venv automatiquement"
}

Write-Host ""
Write-Host "=== Installation de Zer0│GH0ST / Zero-chan (Windows) ===" -ForegroundColor Magenta
Write-Host ""

if ($VoiceOnly) {
  Install-VoiceTools
  Write-Host ""
  Ok "Prérequis Zero-chan installés."
  exit 0
}

Install-CoreTools

if (-not $Minimal) {
  Install-VoiceTools
} else {
  Warn "Mode minimal : voix et média ne sont pas installés."
}

if (-not $SkipBuild) {
  Refresh-Path
  if (-not (Has-Command "cargo.exe")) { Die "Cargo est introuvable après l'installation de Rust." }
  Push-Location $ProjectRoot
  try {
    Info "Compilation de Zer0 (release)…"
    $code = Invoke-NativeCommand "cargo.exe" @("build", "--release")
    if ($code -ne 0) { Die "La compilation Cargo a échoué." }
  } finally {
    Pop-Location
  }
}

Write-Host ""
Ok "Installation terminée."
Info "Binaire : $(Join-Path $ProjectRoot 'target\release\zer0.exe')"
if (-not $Minimal) {
  Info "Dans la TUI : F8 active Zero-chan, F10 active le micro."
}
