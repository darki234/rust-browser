; Установщик Rust Browser (Inno Setup 6). Собирается скриптом installer\build.ps1:
;   powershell -ExecutionPolicy Bypass -File installer\build.ps1
; Ставит для текущего пользователя, без прав администратора, в ту же папку, что и
; установка из самого браузера: %LOCALAPPDATA%\Programs\Rust Browser.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#define AppName "Rust Browser"
#define AppExe "Rust Browser.exe"
#define Root ".."

[Setup]
AppId={{5A770DC9-4AC1-401E-9D7E-397678573F84}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Rust Browser
AppPublisherURL=https://github.com/darki234/rust-browser
AppSupportURL=https://github.com/darki234/rust-browser/issues
AppUpdatesURL=https://github.com/darki234/rust-browser/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\{#AppName}
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
WizardStyle=modern
SetupIconFile={#Root}\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
OutputDir={#Root}\dist
OutputBaseFilename=RustBrowser-Setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
; открытый браузер закрывается на время установки и открывается снова
CloseApplications=yes
RestartApplications=yes

[Languages]
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"

[Types]
Name: "full"; Description: "Полная установка"
Name: "compact"; Description: "Без видеообоев"
Name: "custom"; Description: "Выборочная"; Flags: iscustom

[Components]
Name: "browser"; Description: "Rust Browser"; Types: full compact custom; Flags: fixed
Name: "walls"; Description: "Живые видеообои для новой вкладки (около 120 МБ)"; Types: full

[Tasks]
Name: "startmenu"; Description: "Ярлык в меню «Пуск»"
Name: "desktopicon"; Description: "Ярлык на рабочем столе"

[Files]
Source: "{#Root}\target\release\rust-browser.exe"; DestDir: "{app}"; DestName: "{#AppExe}"; Components: browser; Flags: ignoreversion
Source: "{#Root}\assets\walls\*.webm"; DestDir: "{app}\wallpapers"; Components: walls; Flags: ignoreversion
Source: "{#Root}\assets\walls\credits.json"; DestDir: "{app}\wallpapers"; Components: walls; Flags: ignoreversion

[Icons]
Name: "{userprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: startmenu
Name: "{userdesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "Запустить Rust Browser"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; если браузер делали основным — снять его регистрацию в Windows
Filename: "{app}\{#AppExe}"; Parameters: "--unregister"; Flags: runhidden waituntilterminated; RunOnceId: "Unregister"

[UninstallDelete]
; остатки обновлений (старые exe, переименованные при замене)
Type: files; Name: "{app}\*.old"

[Code]
const
  WebView2Key = 'Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  WebView2Url = 'https://go.microsoft.com/fwlink/p/?LinkId=2124703';

var
  DownloadPage: TDownloadWizardPage;

{ Движок WebView2 есть почти везде на Windows 10 и 11, но не на всех — проверяем. }
function HasVersion(Root: Integer; Key: String): Boolean;
var
  V: String;
begin
  Result := RegQueryStringValue(Root, Key, 'pv', V) and (V <> '') and (V <> '0.0.0.0');
end;

function WebView2Installed: Boolean;
begin
  Result := HasVersion(HKLM, 'SOFTWARE\WOW6432Node\' + WebView2Key)
    or HasVersion(HKLM, 'SOFTWARE\' + WebView2Key)
    or HasVersion(HKCU, 'Software\' + WebView2Key);
end;

procedure InitializeWizard;
begin
  DownloadPage := CreateDownloadPage('Движок WebView2',
    'Rust Browser работает на движке Microsoft Edge WebView2. Его нет на этом компьютере — скачиваем у Microsoft.', nil);
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Code: Integer;
begin
  Result := True;
  if (CurPageID = wpReady) and not WebView2Installed then
  begin
    DownloadPage.Clear;
    DownloadPage.Add(WebView2Url, 'MicrosoftEdgeWebview2Setup.exe', '');
    DownloadPage.Show;
    try
      try
        DownloadPage.Download;
        if not Exec(ExpandConstant('{tmp}\MicrosoftEdgeWebview2Setup.exe'), '/silent /install', '', SW_HIDE, ewWaitUntilTerminated, Code) or not WebView2Installed then
          Result := MsgBox('Не удалось поставить движок WebView2. Без него браузер не запустится.' + #13#10 +
            'Продолжить установку всё равно?', mbConfirmation, MB_YESNO) = IDYES;
      except
        Result := MsgBox('Не удалось скачать движок WebView2: ' + GetExceptionMessage + #13#10 +
          'Продолжить установку всё равно?', mbConfirmation, MB_YESNO) = IDYES;
      end;
    finally
      DownloadPage.Hide;
    end;
  end;
end;
