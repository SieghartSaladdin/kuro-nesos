; Kuro Nesos installer (Inno Setup 6). Built by installer/build.ps1, which passes the version
; and the exe path:  ISCC /DAppVersion=1.2.0 /DSourceExe=..\target\release\kuro-nesos.exe kuro-nesos.iss
; Per-user install (no admin rights): %LOCALAPPDATA%\Programs\Kuro Nesos.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\kuro-nesos.exe"
#endif

[Setup]
AppId={{6B0E2C9A-3F4D-4C1E-9A7B-5D2E8F1C0A11}
AppName=Kuro Nesos
AppVersion={#AppVersion}
AppVerName=Kuro Nesos {#AppVersion}
AppPublisher=SieghartSaladdin
AppPublisherURL=https://github.com/SieghartSaladdin/kuro-nesos
AppSupportURL=https://github.com/SieghartSaladdin/kuro-nesos/issues
AppUpdatesURL=https://github.com/SieghartSaladdin/kuro-nesos/releases
DefaultDirName={localappdata}\Programs\Kuro Nesos
DefaultGroupName=Kuro Nesos
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=KuroNesos-{#AppVersion}-Setup
SetupIconFile=..\assets\kuro-nesos.ico
UninstallDisplayIcon={app}\kuro-nesos.exe
UninstallDisplayName=Kuro Nesos
LicenseFile=..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.18362
; close a running Kuro Nesos before files are replaced
CloseApplications=force
RestartApplications=no

[Tasks]
Name: "autostart"; Description: "Start Kuro Nesos with Windows"; GroupDescription: "Startup:"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{group}\Kuro Nesos"; Filename: "{app}\kuro-nesos.exe"
Name: "{group}\Uninstall Kuro Nesos"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Kuro Nesos"; Filename: "{app}\kuro-nesos.exe"; Tasks: desktopicon

[Registry]
; same value the app's own "Start with Windows" switch writes (model::set_autostart)
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "KuroNesos"; ValueData: """{app}\kuro-nesos.exe"""; Tasks: autostart

[Run]
Filename: "{app}\kuro-nesos.exe"; Description: "{cm:LaunchProgram,Kuro Nesos}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/IM kuro-nesos.exe /F"; Flags: runhidden; RunOnceId: "StopKuroNesos"

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  // the autostart entry may also have been switched on later from inside the app
  if CurUninstallStep = usPostUninstall then
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'KuroNesos');
end;
