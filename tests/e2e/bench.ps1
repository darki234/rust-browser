# Замер против Edge: время до появления окна и память всего дерева процессов
# через 12 секунд после открытия локальной тестовой страницы (без сети).
#   powershell -ExecutionPolicy Bypass -File tests\e2e\bench.ps1 [-Runs 3]
# Edge запускается с отдельным временным профилем и за пределами экрана —
# ваш обычный Edge и его профиль не затрагиваются.
param([int]$Runs = 3)

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Resolve-Path "$here\..\.."
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
Push-Location $root; cargo build --release --quiet; Pop-Location

$ours = "$root\target\release\rust-browser.exe"
$edge = @("${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe", "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
$work = Join-Path $env:TEMP "rust-browser-bench"
$oursData = "$work\ours"; $edgeData = "$work\edge"
New-Item -ItemType Directory -Force $oursData, $edgeData | Out-Null
'{"restore_session":false}' | Set-Content "$oursData\settings.json" -Encoding Ascii
$url = ([Uri](Resolve-Path "$here\fixture\index.html").Path).AbsoluteUri

Add-Type @'
using System; using System.Text; using System.Runtime.InteropServices;
public static class BenchWin {
  public delegate bool Cb(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Cb cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  public static bool HasWindow(uint[] pids, string cls) { bool found = false; EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); if (Array.IndexOf(pids, p) >= 0 && IsWindowVisible(h)) { var c = new StringBuilder(64); GetClassName(h, c, 64); if (c.ToString() == cls) { found = true; return false; } } return true; }, IntPtr.Zero); return found; }
}
'@

function Tree($marker) {
    Get-CimInstance Win32_Process -Filter "Name='msedge.exe' OR Name='msedgewebview2.exe' OR Name='rust-browser.exe'" |
        Where-Object { $_.CommandLine -like "*$marker*" -or ($marker -eq $oursData -and $_.ExecutablePath -eq $ours) }
}

function Run-One($name, $exe, $argv, $marker, $cls) {
    $env:RUST_BROWSER_DATA = $oursData
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--disable-features=CalculateNativeWinOcclusion'
    $clock = [Diagnostics.Stopwatch]::StartNew()
    Start-Process $exe -ArgumentList $argv -WindowStyle Minimized | Out-Null
    $window = $null
    while ($clock.ElapsedMilliseconds -lt 15000) {
        $pids = [uint32[]]@((Tree $marker).ProcessId)
        if ($pids.Count -and [BenchWin]::HasWindow($pids, $cls)) { $window = $clock.ElapsedMilliseconds; break }
        Start-Sleep -Milliseconds 20
    }
    Start-Sleep -Seconds 12
    $procs = @(Tree $marker)
    $mb = ($procs | ForEach-Object { (Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue).PrivateMemorySize64 } | Measure-Object -Sum).Sum / 1MB
    $procs | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 2
    [pscustomobject]@{ Browser = $name; WindowMs = $window; PrivateMB = [math]::Round($mb); Processes = $procs.Count }
}

$results = @()
foreach ($round in 0..$Runs) {
    $a = Run-One 'Rust Browser' $ours @($url) $oursData 'Window Class'
    $b = Run-One 'Edge' $edge @("--user-data-dir=`"$edgeData`"", '--no-first-run', '--no-default-browser-check', '--window-position=-6000,100', $url) $edgeData 'Chrome_WidgetWin_1'
    if ($round -gt 0) { $results += $a, $b }   # нулевой круг — прогрев: создание профилей
}
$results | Format-Table -AutoSize | Out-String
$results | Group-Object Browser | ForEach-Object {
    $w = ($_.Group.WindowMs | Sort-Object)[[int][math]::Floor($_.Count / 2)]
    $m = ($_.Group.PrivateMB | Sort-Object)[[int][math]::Floor($_.Count / 2)]
    '{0}: медиана — окно за {1} мс, память {2} МБ' -f $_.Name, $w, $m
}
