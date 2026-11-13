# Скачивает живые обои с Wikimedia Commons по списку credits.json.
# Все ролики — общественное достояние, CC0 или CC BY (авторы указаны в credits.json и в браузере).
# Видео не хранятся в git: после клонирования запустите этот скрипт один раз.
$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
$list = Get-Content -Raw -Encoding UTF8 (Join-Path $dir 'credits.json') | ConvertFrom-Json
foreach ($item in $list) {
    $target = Join-Path $dir $item.file
    if ((Test-Path $target) -and (Get-Item $target).Length -gt 100000) { "есть    $($item.file)"; continue }
    "качаю  $($item.file)"
    & curl.exe -sSL --retry 3 -A 'RustBrowser/0.2 (wallpaper fetch)' -o $target $item.url
    Start-Sleep -Seconds 2
}
