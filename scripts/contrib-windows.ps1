<#
  contrib-windows.ps1 — publie ton travail sur TON fork, en une commande (Windows).

  À lancer depuis le dossier du projet (celui qui contient `.git`).

  Exemples :
    .\scripts\contrib-windows.ps1 -Message "feat: module 0box"
    .\scripts\contrib-windows.ps1 -Message "fix: bug" -Branch "fix/truc"
    .\scripts\contrib-windows.ps1 -Sync          # récupère les mises à jour de Richerrail

  (Si "l'exécution de scripts est désactivée", lance plutôt : .\scripts\contrib.cmd -Message "...")
#>
param(
  [string]$Message     = "",
  [string]$Branch      = "",
  [string]$ForkUrl     = "https://github.com/darkdober4/zer0gh0st.git",
  [string]$UpstreamUrl = "https://github.com/Richerrail/zer0gh0st.git",
  [switch]$Sync
)

function Info($m) { Write-Host "-> $m" -ForegroundColor Cyan }
function Ok($m)   { Write-Host "OK $m" -ForegroundColor Green }
function Die($m)  { Write-Host "X  $m" -ForegroundColor Red; exit 1 }

# git installé ?
if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
  Die "git introuvable. Installe Git for Windows : https://git-scm.com/download/win"
}

# dans un dépôt git ?
git rev-parse --is-inside-work-tree *> $null
if ($LASTEXITCODE -ne 0) {
  Die "Lance ce script depuis le dossier du projet (celui qui contient .git)."
}

# remotes (ajout idempotent)
$remotes = @(git remote)
if ($remotes -notcontains "upstream") { Info "Ajout du remote 'upstream' (Richerrail)"; git remote add upstream $UpstreamUrl }
if ($remotes -notcontains "fork")     { Info "Ajout du remote 'fork' (ton fork)";       git remote add fork $ForkUrl }

if ($Sync) {
  Info "Récupération des mises à jour de Richerrail"
  git fetch upstream
  git merge upstream/main
  if ($LASTEXITCODE -ne 0) { Die "Conflits pendant le merge — règle-les puis relance." }
  Ok "À jour avec Richerrail/main."
  exit 0
}

# branche
if ([string]::IsNullOrWhiteSpace($Branch))  { $Branch  = "feat/" + (Get-Date -Format "yyyyMMdd-HHmmss") }
if ([string]::IsNullOrWhiteSpace($Message)) { $Message = "feat: " + (Get-Date -Format "yyyy-MM-dd HH:mm") }

$has = @(git branch --list $Branch)
if ($has.Count -gt 0) { git checkout $Branch | Out-Null } else { git checkout -b $Branch | Out-Null }
if ($LASTEXITCODE -ne 0) { Die "Impossible de basculer sur la branche '$Branch'." }

# commit
git add -A
$staged = @(git status --porcelain)
if ($staged.Count -eq 0) { Die "Aucun changement à committer (rien de modifié)." }
git commit -m $Message
if ($LASTEXITCODE -ne 0) { Die "Échec du commit." }
Ok "Commit créé : $Message"

# push vers TON fork
Info "Envoi vers ton fork…"
git push -u fork $Branch
if ($LASTEXITCODE -ne 0) { Die "Échec du push — vérifie ton token/clé GitHub." }
Ok "Poussé sur ton fork."

# lien pour la Pull Request
$owner = (($ForkUrl -replace "https://github.com/", "") -replace "\.git$", "").Split("/")[0]
$url   = "https://github.com/Richerrail/zer0gh0st/compare/main...${owner}:${Branch}?expand=1"
Write-Host ""
Write-Host "Ouvre une Pull Request ici :" -ForegroundColor Yellow
Write-Host "  $url"
