# Сборка всего для релиза: powershell -ExecutionPolicy Bypass -File installer\build.ps1
# Кладёт в папку dist:
#   RustBrowser-Setup-<версия>.exe    — установщик для людей (с видеообоями)
#   RustBrowser-<версия>-portable.exe — сам браузер: по нему обновляются установленные копии
#   RustBrowser-<версия>.apk          — версия для Android (если собрана)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot\.."
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value

Push-Location $root
try {
    $ErrorActionPreference = 'Continue' # cargo пишет ход сборки в stderr
    cargo build --release 2>&1 | Out-Host
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE) { throw 'сборка браузера не удалась' }
} finally { Pop-Location }

$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") |
    Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Нет Inno Setup 6: winget install JRSoftware.InnoSetup' }

$dist = "$root\dist"
New-Item -ItemType Directory -Force $dist | Out-Null
$ErrorActionPreference = 'Continue'
& $iscc "/DAppVersion=$version" "$PSScriptRoot\rust-browser.iss" 2>&1 | Out-Host
$ErrorActionPreference = 'Stop'
if ($LASTEXITCODE) { throw 'сборка установщика не удалась' }

Copy-Item "$root\target\release\rust-browser.exe" "$dist\RustBrowser-$version-portable.exe" -Force
if (Test-Path "$root\Rust Browser.apk") { Copy-Item "$root\Rust Browser.apk" "$dist\RustBrowser-$version.apk" -Force }

Get-ChildItem $dist -Filter "*$version*" | ForEach-Object {
    $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
    '{0,-40} {1,8:N1} МБ  sha256 {2}' -f $_.Name, ($_.Length / 1MB), $hash
}
