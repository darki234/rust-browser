# Сборка Android-версии: .\android\build.ps1  →  «Rust Browser.apk» в корне проекта.
# Gradle для Android не работает с кириллицей в пути, поэтому проект временно
# подключается диском R: (subst), а сборка идёт оттуда.
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not (subst | Select-String '^R:')) { subst R: $repo }
# инструменты Android лежат на диске D (на C мало места)
$env:JAVA_HOME = (Get-ChildItem "D:\Android\jdk" -Directory | Select-Object -First 1).FullName
Push-Location R:\android
try {
    & .\gradlew.bat assembleRelease --console=plain
    if ($LASTEXITCODE -ne 0) { throw 'сборка не удалась' }
} finally { Pop-Location }
Copy-Item R:\android\app\build\outputs\apk\release\app-release.apk (Join-Path $repo 'Rust Browser.apk') -Force
Write-Host 'Готово: Rust Browser.apk'
