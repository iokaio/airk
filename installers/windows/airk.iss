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
ChangesEnvironment=yes
UninstallDisplayName=AIRK CLI and MCP server
UninstallDisplayIcon={app}\runtime\airk.exe
LicenseFile={#AirkRuntime}\LICENSE

[Files]
Source: "{#AirkRuntime}\*"; DestDir: "{app}\runtime"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#AirkInnoLicense}"; DestDir: "{app}\notices\inno-setup"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\AIRK Setup"; Filename: "{app}\runtime\airk.exe"; Parameters: "setup --runtime-source ""{app}\runtime"" --approved-sha256 {#AirkInstallSHA}"

[Code]
function UserPathEntry: String;
begin
  Result := ExpandConstant('{app}\runtime');
  if (Pos(';', Result) > 0) or (Pos(#13, Result) > 0) or
     (Pos(#10, Result) > 0) then
    RaiseException('The AIRK runtime directory cannot be represented in the user PATH.');
end;

function PathOwnerFile: String;
begin
  Result := ExpandConstant('{app}\user-path-owner.txt');
end;

function ReadUserPath(var HadValue: Boolean): String;
begin
  Result := '';
  HadValue := RegValueExists(HKCU, 'Environment', 'Path');
  if HadValue and (not RegQueryStringValue(HKCU, 'Environment', 'Path', Result)) then
    RaiseException('The user PATH is unreadable or is not a string value. It was preserved.');
  if (Length(Result) > 32767) or (Pos(#0, Result) > 0) or
     (Pos(#13, Result) > 0) or (Pos(#10, Result) > 0) then
    RaiseException('The user PATH exceeds its supported representation. It was preserved.');
end;

function PathParts(Value: String): TArrayOfString;
var
  At, Count: Integer;
begin
  SetArrayLength(Result, 0);
  repeat
    At := Pos(';', Value);
    Count := GetArrayLength(Result);
    SetArrayLength(Result, Count + 1);
    if At = 0 then begin
      Result[Count] := Value;
      Break;
    end;
    Result[Count] := Copy(Value, 1, At - 1);
    Delete(Value, 1, At);
  until False;
end;

function ComparablePathEntry(Value: String): String;
begin
  { Compare literal directories only. REG_SZ does not expand percent variables,
    and RegQueryStringValue alone does not distinguish it from REG_EXPAND_SZ.
    https://learn.microsoft.com/en-us/windows/win32/sysinfo/registry-value-types }
  Result := Trim(Value);
  if Length(Result) >= 2 then
    if (Result[1] = '"') and (Result[Length(Result)] = '"') then
      Result := Copy(Result, 2, Length(Result) - 2);
  StringChangeEx(Result, '/', '\', True);
  while Length(Result) > 3 do begin
    if Result[Length(Result)] <> '\' then Break;
    Delete(Result, Length(Result), 1);
  end;
end;

function HasPathEntry(const Value, Entry: String): Boolean;
var
  Parts: TArrayOfString;
  I: Integer;
begin
  Result := False;
  Parts := PathParts(Value);
  for I := 0 to GetArrayLength(Parts) - 1 do
    if CompareText(ComparablePathEntry(Parts[I]), ComparablePathEntry(Entry)) = 0 then begin
      Result := True;
      Exit;
    end;
end;

function WithPathEntry(const Before, Entry: String): String;
begin
  Result := Entry;
  if Before <> '' then Result := Result + ';' + Before;
  if Length(Result) > 32767 then
    RaiseException('Adding AIRK would exceed the user PATH limit. Existing PATH was preserved.');
end;

function ReadPathOwner(var Lines: TArrayOfString): Boolean;
var
  Size: Int64;
begin
  Result := FileExists(PathOwnerFile);
  if not Result then Exit;
  if (not FileSize64(PathOwnerFile, Size)) or (Size > 262144) or
     (not LoadStringsFromFile(PathOwnerFile, Lines)) then
    RaiseException('The AIRK user PATH ownership receipt is unreadable. Existing PATH was preserved.');
  if GetArrayLength(Lines) <> 6 then
    RaiseException('The AIRK user PATH ownership receipt is incomplete. Existing PATH was preserved.');
  if (Lines[0] <> 'airk-user-path/1') or
     ((Lines[1] <> 'pending') and (Lines[1] <> 'owned')) or
     (CompareText(Lines[2], UserPathEntry) <> 0) or ((Lines[3] <> '0') and (Lines[3] <> '1')) or
     (Lines[5] <> WithPathEntry(Lines[4], Lines[2])) then
    RaiseException('The AIRK user PATH ownership receipt changed. Existing PATH was preserved.');
end;

procedure WriteUserPath(const Expected: String; ExpectedExists: Boolean;
  const After: String; DeleteEmpty: Boolean);
var
  Current: String;
  Exists: Boolean;
begin
  Current := ReadUserPath(Exists);
  if (Current <> Expected) or (Exists <> ExpectedExists) then
    RaiseException('The user PATH changed during AIRK setup. Retry after reviewing that change.');
  if DeleteEmpty and (After = '') then begin
    if not RegDeleteValue(HKCU, 'Environment', 'Path') then
      RaiseException('The owned AIRK user PATH entry could not be removed.');
  end else begin
    { Inno preserves an existing REG_EXPAND_SZ with RegWriteStringValue.
      A missing value becomes REG_SZ, appropriate for this literal directory.
      https://jrsoftware.org/ishelp/topic_isxfunc_regwritestringvalue.htm }
    if not RegWriteStringValue(HKCU, 'Environment', 'Path', After) then
      RaiseException('The AIRK user PATH update could not be saved.');
  end;
  Current := ReadUserPath(Exists);
  if (Current <> After) or (Exists <> (not (DeleteEmpty and (After = '')))) then
    RaiseException('The AIRK user PATH update could not be verified.');
end;

procedure AddOwnedUserPath;
var
  Before, Entry: String;
  HadValue: Boolean;
  Lines: TArrayOfString;
begin
  Entry := UserPathEntry;
  Before := ReadUserPath(HadValue);
  if ReadPathOwner(Lines) then begin
    if Lines[1] = 'owned' then begin
      if HasPathEntry(Before, Entry) then Exit;
    end else if (Before <> Lines[4]) or (HadValue <> (Lines[3] = '1')) then
      RaiseException('An earlier AIRK PATH update was interrupted. Existing PATH and its receipt were preserved for review.');
  end else if HasPathEntry(Before, Entry) then begin
    Log('The AIRK directory was already on the user PATH; no ownership was claimed.');
    Exit;
  end;
  SetArrayLength(Lines, 6);
  Lines[0] := 'airk-user-path/1';
  Lines[1] := 'pending';
  Lines[2] := Entry;
  if HadValue then Lines[3] := '1' else Lines[3] := '0';
  Lines[4] := Before;
  Lines[5] := WithPathEntry(Before, Entry);
  if not SaveStringsToUTF8File(PathOwnerFile, Lines, False) then
    RaiseException('AIRK could not record its PATH update intent. Existing PATH was preserved.');
  WriteUserPath(Before, HadValue, Lines[5], False);
  Lines[1] := 'owned';
  if not SaveStringsToUTF8File(PathOwnerFile, Lines, False) then
    RaiseException('AIRK could not confirm ownership of its PATH update. Its pending receipt was preserved.');
end;

procedure RemoveOwnedUserPath;
var
  Lines, Parts: TArrayOfString;
  Before, After, Entry: String;
  HadValue: Boolean;
  I, ExactCount, Kept: Integer;
begin
  if not ReadPathOwner(Lines) then Exit;
  if Lines[1] <> 'owned' then
    RaiseException('The AIRK PATH update has no completed ownership receipt. Existing PATH and packaged files were preserved.');
  Entry := Lines[2];
  Before := ReadUserPath(HadValue);
  Parts := PathParts(Before);
  After := '';
  ExactCount := 0;
  Kept := 0;
  for I := 0 to GetArrayLength(Parts) - 1 do begin
    if Parts[I] = Entry then ExactCount := ExactCount + 1
    else begin
      if Kept > 0 then After := After + ';';
      After := After + Parts[I];
      Kept := Kept + 1;
    end;
  end;
  if ExactCount = 0 then
    Log('The exact owned AIRK PATH entry is already absent; manually edited entries were preserved.')
  else begin
    if ExactCount <> 1 then
      RaiseException('The AIRK PATH entry is duplicated and ownership is ambiguous. Existing PATH was preserved.');
    WriteUserPath(Before, HadValue, After, (Lines[3] = '0') and (Kept = 0));
  end;
  if not DeleteFile(PathOwnerFile) then
    RaiseException('The AIRK PATH update completed, but its ownership receipt could not be retired.');
end;

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
    AddOwnedUserPath;
  end;
end;

function InitializeUninstall: Boolean;
var
  VerifiedBinary: String;
begin
  Result := False;
  try
    { This event precedes confirmation. Validate read-only; Cancel changes nothing. }
    VerifiedBinary := NativeBinary;
    Result := True;
  except
    MsgBox(GetExceptionMessage, mbError, MB_OK);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ExitCode: Integer;
begin
  if CurUninstallStep = usUninstall then begin
    try
      { Inno calls this after confirmation and before its tracked-file removal.
        The native owner stops only its proven installation and preserves indexes. }
      if not Exec(NativeBinary, 'uninstall --yes', ExpandConstant('{app}\runtime'),
        SW_SHOW, ewWaitUntilTerminated, ExitCode) then
        RaiseException('Native AIRK uninstall could not start; packaged files were preserved.');
      if ExitCode <> 0 then
        RaiseException('Native AIRK uninstall did not complete; packaged files were preserved. Review the native failure before trying again.');
      RemoveOwnedUserPath;
    except
      MsgBox(GetExceptionMessage, mbError, MB_OK);
      { Abort exits Uninstall from this event; merely displaying an error would
        let tracked-file removal continue. This is supported by pinned Inno 6.7.1.
        https://jrsoftware.org/ishelp/topic_isxfunc_abort.htm }
      Abort;
    end;
  end;
end;
