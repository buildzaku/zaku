[Setup]
AppId={{f80b4ce0-da83-424f-92ec-e17462766875}
AppName=Zaku
AppVerName=Zaku {#Version}
AppVersion={#Version}
VersionInfoVersion={#Version}
AppPublisher=Zaku
AppPublisherURL=https://zaku.dev/
AppSupportURL=https://zaku.dev/
AppUpdatesURL=https://zaku.dev/
DefaultDirName={autopf}\Zaku
DefaultGroupName=Zaku
DisableProgramGroupPage=yes
DisableReadyPage=yes
AllowNoIcons=yes
OutputDir={#OutputDir}
OutputBaseFilename=Zaku-{#Version}-{#Architecture}
Compression=lzma2
SolidCompression=yes
SetupMutex=Zaku-Setup-Mutex
SetupArchitecture=x64
SetupIconFile={#SourceDir}\app-icon.ico
UninstallDisplayIcon={app}\Zaku.exe
MinVersion=10.0.22000
SourceDir={#SourceDir}
WizardStyle=modern dynamic
CloseApplications=yes
PrivilegesRequired=lowest

#if Architecture == "aarch64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible and not arm64
ArchitecturesInstallIn64BitMode=x64compatible and not arm64
#endif

[UninstallDelete]
Type: filesandordirs; Name: "{app}\tools"
Type: filesandordirs; Name: "{app}\updates"
Type: filesandordirs; Name: "{app}\install"
Type: filesandordirs; Name: "{app}\old"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "Zaku.exe"; DestDir: "{code:GetInstallDir}"; Flags: ignoreversion
Source: "tools\updater_windows.exe"; DestDir: "{app}\tools"; Flags: ignoreversion

[Icons]
Name: "{group}\Zaku"; Filename: "{app}\Zaku.exe"; AppUserModelID: "dev.zaku.Zaku"
Name: "{autodesktop}\Zaku"; Filename: "{app}\Zaku.exe"; Tasks: desktopicon; AppUserModelID: "dev.zaku.Zaku"

[Run]
Filename: "{app}\Zaku.exe"; Description: "{cm:LaunchProgram,Zaku}"; Flags: nowait postinstall; Check: WizardNotSilent

[Code]
function WizardNotSilent(): Boolean;
begin
  Result := not WizardSilent();
end;

function SwitchHasValue(Name: string; Value: string): Boolean;
begin
  Result := CompareText(ExpandConstant('{param:' + Name + '}'), Value) = 0;
end;

function IsUpdating(): Boolean;
begin
  Result := SwitchHasValue('update', 'true') and WizardSilent();
end;

function GetInstallDir(Param: String): String;
begin
  if IsUpdating() then
    Result := ExpandConstant('{app}\install')
  else
    Result := ExpandConstant('{app}');
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  UpdateDirectory: String;
begin
  if (CurStep = ssPostInstall) and IsUpdating() then
  begin
    UpdateDirectory := ExpandConstant('{app}\updates');
    if not ForceDirectories(UpdateDirectory) then
      RaiseException('Could not create the update directory');
    if not SaveStringToFile(UpdateDirectory + '\versions.txt', '{#Version}', False) then
      RaiseException('Could not write the update marker');
  end;
end;
