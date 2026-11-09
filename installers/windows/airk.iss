; SPDX-License-Identifier: Apache-2.0
; The package owns only its per-user bootstrap payload. Native AIRK owns the
; managed runtime, upgrades, service, MCP enrollment and retained index data.
#ifndef AirkVersion
  #error Build through scripts/package-standalone-windows.py
#endif

[Setup]
AppId={{87B12FF2-FF1A-4E65-AD6C-53CB538E6303}
AppName=AIRK CLI and MCP server
AppVersion={#AirkVersion}
AppPublisher=Ioka LLC
AppPublisherURL=https://github.com/iokaio/airk
DefaultDirName={localappdata}\AIRK\standalone
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#AirkOutput}
OutputBaseFilename={#AirkArtifact}
SignedUninstaller=yes
SignedUninstallerDir={#AirkUninstallers}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
CloseApplications=no
RestartApplications=no
UninstallDisplayName=AIRK CLI and MCP server
UninstallDisplayIcon={app}\runtime\airk.exe
LicenseFile={#AirkRuntime}\LICENSE

[Files]
Source: "{#AirkRuntime}\*"; DestDir: "{app}\runtime"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#AirkInnoLicense}"; DestDir: "{app}\notices\inno-setup"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\AIRK Setup"; Filename: "{app}\runtime\airk.exe"; Parameters: "setup --runtime-source ""{app}\runtime"" --approved-sha256 {#AirkInstallSHA}"

[Code]
function NativeBinary: String;
begin
  Result := ExpandConstant('{app}\runtime\airk.exe');
  if (not FileExists(Result)) or
     (CompareText(GetSHA256OfFile(Result), '{#AirkBinarySHA}') <> 0) then
    RaiseException('The packaged AIRK runtime changed. Its files were preserved; download the verified installer again.');
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ExitCode: Integer;
  Parameters: String;
begin
  if CurStep = ssPostInstall then begin
    Parameters := 'setup --runtime-source "' + ExpandConstant('{app}\runtime') +
      '" --approved-sha256 {#AirkInstallSHA}';
    if not Exec(NativeBinary, Parameters, ExpandConstant('{app}\runtime'),
      SW_SHOW, ewWaitUntilTerminated, ExitCode) then
      RaiseException('AIRK setup could not start. The packaged files remain available through AIRK Setup.');
    if ExitCode <> 0 then
      RaiseException('Native AIRK setup did not complete. Review its message and rerun AIRK Setup; retained indexes and settings were not removed.');
  end;
end;

function InitializeUninstall: Boolean;
var
  ExitCode: Integer;
begin
  Result := False;
  try
    { Add/Remove Programs is the explicit uninstall action. The native owner
      stops/removes only its proven installation and preserves index data. }
    if not Exec(NativeBinary, 'uninstall --yes', ExpandConstant('{app}\runtime'),
      SW_SHOW, ewWaitUntilTerminated, ExitCode) then
      RaiseException('Native AIRK uninstall could not start; packaged files were preserved.');
    if ExitCode <> 0 then
      RaiseException('Native AIRK uninstall did not complete; packaged files were preserved. Review the native failure before trying again.');
    Result := True;
  except
    MsgBox(GetExceptionMessage, mbError, MB_OK);
  end;
end;
