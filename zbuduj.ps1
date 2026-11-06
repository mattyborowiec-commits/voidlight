# Voidlight: buduje instalator bez uprawnien administratora.
# Wszystkie narzedzia ida do folderu .toolchain obok projektu (mozna go potem usunac).
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$root = $PSScriptRoot
$tc = Join-Path $root '.toolchain'
New-Item -ItemType Directory -Force $tc | Out-Null
Start-Transcript -Path (Join-Path $tc 'build.log') -Force | Out-Null

function Step($t) { Write-Host "`n=== $t ===" -ForegroundColor Cyan }

# 1. Node.js (wersja przenosna)
Step 'Node.js'
$node = Join-Path $tc 'node'
if (-not (Test-Path "$node\node.exe")) {
    $zip = Join-Path $tc 'node.zip'
    Invoke-WebRequest 'https://nodejs.org/dist/v20.18.0/node-v20.18.0-win-x64.zip' -OutFile $zip
    Expand-Archive $zip -DestinationPath $tc -Force
    Rename-Item (Join-Path $tc 'node-v20.18.0-win-x64') $node
    Remove-Item $zip
}

# 2. Kompilator MinGW (w64devkit, przenosny)
Step 'Kompilator (w64devkit)'
$mingw = Join-Path $tc 'w64devkit'
if (-not (Test-Path "$mingw\bin\gcc.exe")) {
    $rel = Invoke-RestMethod 'https://api.github.com/repos/skeeto/w64devkit/releases/latest' -Headers @{ 'User-Agent' = 'voidlight' }
    $asset = $rel.assets | Where-Object { $_.name -match '^w64devkit-x64-.*\.exe$' } | Select-Object -First 1
    if (-not $asset) { throw 'Nie znaleziono w64devkit do pobrania.' }
    $sfx = Join-Path $tc 'w64devkit.exe'
    Invoke-WebRequest $asset.browser_download_url -OutFile $sfx
    & $sfx -y "-o$tc" | Out-Null
    Remove-Item $sfx
}

# 3. Rust (toolchain GNU, instalacja w folderze projektu)
Step 'Rust'
$env:RUSTUP_HOME = Join-Path $tc 'rustup'
$env:CARGO_HOME = Join-Path $tc 'cargo'
$env:Path = "$mingw\bin;$node;$env:CARGO_HOME\bin;$env:Path"
# w64devkit nie ma biblioteki libgcc_eh, wiec uzywamy bibliotek dolaczonych do Rusta.
$env:RUSTFLAGS = '-C link-self-contained=yes'
if (-not (Test-Path "$env:CARGO_HOME\bin\cargo.exe")) {
    $init = Join-Path $tc 'rustup-init.exe'
    Invoke-WebRequest 'https://win.rustup.rs/x86_64' -OutFile $init
    & $init -y --default-host x86_64-pc-windows-gnu --default-toolchain stable-x86_64-pc-windows-gnu --profile minimal --no-modify-path
    Remove-Item $init
}

# 4. Budowanie
Step 'Instalacja pakietow (npm)'
Set-Location $root
& "$node\npm.cmd" install
if ($LASTEXITCODE -ne 0) { throw 'npm install nie powiodlo sie.' }

Step 'Budowanie launchera (pierwszy raz kilka-kilkanascie minut)'
& "$node\npm.cmd" run build
if ($LASTEXITCODE -ne 0) { throw 'Budowanie nie powiodlo sie. Wyslij mi plik .toolchain\build.log' }

# 5. Uruchomienie instalatora
$exe = Get-ChildItem (Join-Path $root 'src-tauri\target\release\bundle\nsis') -Filter *.exe | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $exe) { throw 'Nie znaleziono instalatora.' }
Step "Gotowe: $($exe.FullName)"
Copy-Item $exe.FullName (Join-Path $root 'Voidlight-instalator.exe') -Force
Stop-Transcript | Out-Null
Start-Process (Join-Path $root 'Voidlight-instalator.exe')
