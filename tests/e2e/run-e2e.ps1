# Сквозные проверки браузера одной командой:
#   powershell -ExecutionPolicy Bypass -File tests\e2e\run-e2e.ps1 [-Release]
# Собирает браузер, запускает отдельную тестовую копию (своя папка данных, окно за
# пределами экрана, чтобы не мешать), прогоняет e2e.mjs и всё за собой убирает.
param([switch]$Release, [int]$Port = 9333)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Resolve-Path "$here\..\.."
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

Push-Location $root
$profileName = if ($Release) { 'release' } else { 'debug' }
cargo build $(if ($Release) { '--release' }) --quiet
if ($LASTEXITCODE) { Pop-Location; throw 'сборка не удалась' }
Pop-Location
$exe = "$root\target\$profileName\rust-browser.exe"

$data = Join-Path $env:TEMP ("rust-browser-e2e-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory $data | Out-Null
'{"restore_session":false}' | Set-Content "$data\settings.json" -Encoding Ascii
$env:RUST_BROWSER_DATA = $data
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port --disable-features=CalculateNativeWinOcclusion"

Add-Type @'
using System; using System.Text; using System.Runtime.InteropServices;
public static class E2EWin {
  public delegate bool Cb(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Cb cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  public static IntPtr Find(uint pid) { IntPtr found = IntPtr.Zero; EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); var c = new StringBuilder(64); GetClassName(h, c, 64); if (p == pid && c.ToString() == "Window Class") { found = h; return false; } return true; }, IntPtr.Zero); return found; }
}
'@

$browser = Start-Process $exe -PassThru -WindowStyle Minimized
try {
    # окно — за пределы экрана и без активации: проверки не мешают работать
    $hwnd = [IntPtr]::Zero
    for ($i = 0; $i -lt 50 -and $hwnd -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 100; $hwnd = [E2EWin]::Find([uint32]$browser.Id) }
    [void][E2EWin]::ShowWindow($hwnd, 4)
    [void][E2EWin]::SetWindowPos($hwnd, [IntPtr]1, -6000, 100, 1400, 900, 0x0010)

    $fixture = ([Uri](Resolve-Path "$here\fixture\index.html").Path).AbsoluteUri
    node "$here\e2e.mjs" $Port $fixture $exe
    $code = $LASTEXITCODE
}
finally {
    Get-CimInstance Win32_Process -Filter "Name='rust-browser.exe' OR Name='msedgewebview2.exe'" |
        Where-Object { $_.ExecutablePath -eq $exe -or $_.CommandLine -like "*$data*" } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 2
    Remove-Item $data -Recurse -Force -ErrorAction SilentlyContinue
}
exit $code
