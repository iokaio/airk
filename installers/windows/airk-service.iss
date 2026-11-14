; SPDX-License-Identifier: Apache-2.0
; AIRK Service (split plan section 9; Phase 7). Per user, no elevation.
; The installer only places the fixed section 9.5 layout and hands every other
; step to the signed airk image's private setup role (U15, U16, U33):
;   bin\airkctl.exe, service\airk.exe, notices\, payload-inventory.json.
; Predecessor removal, login startup, certificate trust, the install record
; and the install pointer belong to setup, never to this script.
;
; Build through scripts/build-local-service-installer.py, which defines:
;   AirkVersion, AirkPayload (the staged layout), AirkOutput, AirkArtifact,
;   AirkInventorySHA (payload-inventory.json), AirkBinarySHA (service\airk.exe),
;   AirkCtlBinarySHA (bin\airkctl.exe), and, for a signed build,
;   AirkUninstallers (the folder of the signed uninstaller).
#ifndef AirkVersion
  #error Build through scripts/build-local-service-installer.py
#endif

[Setup]
; A new AppId: the pre-split installer's {87B12FF2-...} entry is retired by
; setup through its receipts, never upgraded in place (inventory B2).
AppId={{1A421FBD-1C33-47AD-9770-AE490F2737CF}
AppName=AIRK Service
AppVersion={#AirkVersion}
AppPublisher=Ioka LLC
AppPublisherURL=https://airk.ioka.io
DefaultDirName={localappdata}\AIRK Service\program
DisableDirPage=no
DisableStartupPrompt=yes
UsePreviousAppDir=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#AirkOutput}
OutputBaseFilename={#AirkArtifact}
#ifdef AirkUninstallers
SignedUninstaller=yes
SignedUninstallerDir={#AirkUninstallers}
#endif
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
CloseApplications=no
RestartApplications=no
ChangesEnvironment=yes
UninstallDisplayName=AIRK Service
UninstallDisplayIcon={app}\service\airk.exe
LicenseFile={#AirkPayload}\notices\LICENSE

[Files]
Source: "{#AirkPayload}\service\airk.exe"; DestDir: "{app}\service"; Flags: ignoreversion; Check: ShouldPlacePayload
Source: "{#AirkPayload}\bin\airkctl.exe"; DestDir: "{app}\bin"; Flags: ignoreversion; Check: ShouldPlacePayload
Source: "{#AirkPayload}\notices\*"; DestDir: "{app}\notices"; Flags: ignoreversion recursesubdirs createallsubdirs; Check: ShouldPlacePayload
Source: "{#AirkPayload}\payload-inventory.json"; DestDir: "{app}"; Flags: ignoreversion; Check: ShouldPlacePayload
; The same bytes, extracted to {tmp} for the plan and preflight that run
; before any file is placed.
Source: "{#AirkPayload}\service\airk.exe"; DestName: "airk-setup.exe"; Flags: dontcopy
Source: "{#AirkPayload}\payload-inventory.json"; DestName: "setup-payload-inventory.json"; Flags: dontcopy

[Icons]
Name: "{userprograms}\AIRK Admin"; Filename: "{app}\bin\airkctl.exe"; Parameters: "admin open"; Comment: "Open the AIRK admin pages in your browser"

[Code]
const
  AppKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{1A421FBD-1C33-47AD-9770-AE490F2737CF}_is1';
  OperationLifetimeMs = 600000;
  { The default root for every location: separate from the pre-split
    %LOCALAPPDATA%\AIRK root and created private (RestrictToUser). }
  DefaultRootName = 'AIRK Service';

type
  TAirkFileTime = record
    Low: Cardinal;
    High: Cardinal;
  end;

procedure GetSystemTimeAsFileTime(var Time: TAirkFileTime);
  external 'GetSystemTimeAsFileTime@kernel32.dll stdcall';

function WaitForSingleObject(Handle: THandle; Milliseconds: Cardinal): Cardinal;
  external 'WaitForSingleObject@kernel32.dll stdcall';
function ReleaseMutex(Handle: THandle): Boolean;
  external 'ReleaseMutex@kernel32.dll stdcall';
function CloseHandle(Handle: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';

var
  DataPage: TInputDirWizardPage;
  DisclosurePage: TWizardPage;
  DisclosureMemo: TNewMemo;
  AcknowledgeBox: TNewCheckBox;
  { Shown while setup --plan runs, so a slow check does not look like a hang. }
  PlanProgressPage: TOutputMarqueeProgressWizardPage;
  NeedsAcknowledgement: Boolean;
  SetupOperation: String;
  PrivateFolder: String;
  ProjectionOperation: String;
  ProjectionOriginalSHA: String;
  FinalizationFailed: Boolean;
  FinalizationError: String;
  UninstallOperationSHA: String;
  RecoveryOperation: String;
  RecoveryPlacement: Boolean;
  RecoveryCompletion: Boolean;
  WrapperLease: THandle;

{ Preserve the child exit code when translating the cross-platform spelling.
  Windows command-line quoting doubles runs of backslashes before a quote. }
function CopyFileW(Existing, Destination: String; FailIfExists: Boolean): Boolean;
  external 'CopyFileW@kernel32.dll stdcall';

procedure ExitProcess(Code: Cardinal);
  external 'ExitProcess@kernel32.dll stdcall';

function QuoteArgument(const Value: String): String;
var
  I, J, Slashes: Integer;
begin
  Result := '"';
  Slashes := 0;
  for I := 1 to Length(Value) do begin
    if Value[I] = '\' then Slashes := Slashes + 1
    else begin
      if Value[I] = '"' then begin
        for J := 1 to Slashes * 2 + 1 do Result := Result + '\';
      end else
        for J := 1 to Slashes do Result := Result + '\';
      Result := Result + Value[I];
      Slashes := 0;
    end;
  end;
  for J := 1 to Slashes * 2 do Result := Result + '\';
  Result := Result + '"';
end;

{ ---------- Time, identity and canonical JSON (contracts section 5.5) ---------- }

{ Unix milliseconds from the system FILETIME (100 ns since 1601). Every
  literal fits 32 bits; the arithmetic is Int64. }
function UnixMillis: Int64;
var
  Time: TAirkFileTime;
  Value, Offset: Int64;
begin
  GetSystemTimeAsFileTime(Time);
  Value := Time.High;
  Value := Value * 65536 * 65536 + Time.Low;
  Offset := 11644473;
  Offset := Offset * 1000000 + 600000;
  Result := Value div 10000 - Offset;
end;

function NewOperationId: String;
begin
  Result := Lowercase(Copy(GetSHA256OfString(IntToStr(UnixMillis) + '|' +
    ExpandConstant('{tmp}') + '|' + IntToStr(Random(2147483647))), 1, 32));
end;

{ serde_json-compatible UTF-8: escape controls, quotes and backslash only. }
function JsonString(Value: String): String;
var
  I, C: Integer;
begin
  Result := '"';
  I := 1;
  while I <= Length(Value) do begin
    C := Ord(Value[I]);
    case C of
      8: Result := Result + '\b';
      9: Result := Result + '\t';
      10: Result := Result + '\n';
      12: Result := Result + '\f';
      13: Result := Result + '\r';
      34: Result := Result + '\"';
      92: Result := Result + '\\';
    else
      if C < 32 then
        Result := Result + '\u00' + Copy('0123456789abcdef', C div 16 + 1, 1) +
          Copy('0123456789abcdef', C mod 16 + 1, 1)
      else if (C >= $D800) and (C <= $DBFF) then begin
        if I = Length(Value) then RaiseException('Invalid Unicode location.');
        if (Ord(Value[I + 1]) < $DC00) or (Ord(Value[I + 1]) > $DFFF) then
          RaiseException('Invalid Unicode location.');
        Result := Result + Value[I] + Value[I + 1];
        I := I + 1;
      end else if (C >= $DC00) and (C <= $DFFF) then
        RaiseException('Invalid Unicode location.')
      else Result := Result + Value[I];
    end;
    I := I + 1;
  end;
  Result := Result + '"';
end;

function JsonBool(Value: Boolean): String;
begin
  if Value then Result := 'true' else Result := 'false';
end;

{ ---------- Locations (U39) ---------- }

function DataRoot: String;
begin
  Result := RemoveBackslashUnlessRoot(DataPage.Values[0]);
end;

function CacheRoot: String;
begin
  Result := RemoveBackslashUnlessRoot(DataPage.Values[1]);
end;

function LocationsJson(const ProgramDir, Data, Cache: String): String;
begin
  Result := '{"program":' + JsonString(ProgramDir) +
    ',"data":' + JsonString(Data + '\data\indexes') +
    ',"state":' + JsonString(Data + '\state\service') +
    ',"cache":' + JsonString(Cache + '\cache\service') +
    ',"logs":' + JsonString(Cache + '\logs\service') + '}';
end;

// Remove the inherited entries of a folder this installer just created and
// grant only this user, SYSTEM and Administrators. Needs no elevation: the
// user owns the folder it created.
procedure RestrictToUser(const Folder: String);
var
  Code: Integer;
  Account: String;
begin
  Account := GetEnv('USERDOMAIN');
  if Account <> '' then Account := Account + '\';
  Account := Account + GetUserNameString;
  if not Exec(ExpandConstant('{sys}\icacls.exe'), '"' + Folder +
    '" /inheritance:r /grant:r "' + Account + ':(OI)(CI)F" "*S-1-5-18:(OI)(CI)F"' +
    ' "*S-1-5-32-544:(OI)(CI)F" /q', '', SW_HIDE, ewWaitUntilTerminated, Code)
    or (Code <> 0) then
    RaiseException('AIRK could not restrict ' + Folder +
      ' to this account (icacls exit ' + IntToStr(Code) + ').');
end;

// The setup temporary folder inherits the user's Temp ACL, which may grant
// other accounts access (for example a sandbox group). Setup reads its
// operation only from an owner-private file, so every operation folder and
// the setup image live below one restricted folder. Setup verifies the
// result itself and fails closed.
function SetupPrivateFolder: String;
begin
  if PrivateFolder = '' then
  begin
    PrivateFolder := ExpandConstant('{localappdata}\airk-setup-recovery-') + NewOperationId;
    if not CreateDir(PrivateFolder) then
      RaiseException('AIRK could not create its private setup folder ' + PrivateFolder);
    RestrictToUser(PrivateFolder);
  end;
  Result := PrivateFolder;
end;

{ The relay exits before Inno's normal deinitializer. Inno 6.7.1 has only
  extracted its unused 64-bit helper here. Retire that exact owned file and
  empty directories; preserve anything unexpected or still open. }
procedure ExitRelay(Code: Cardinal);
begin
  if FileExists(ExpandConstant('{tmp}\_isetup\_setup64.tmp')) then
    if not DeleteFile(ExpandConstant('{tmp}\_isetup\_setup64.tmp')) then
      Log('The relay helper is still in use; its temporary file was preserved.');
  RemoveDir(ExpandConstant('{tmp}\_isetup'));
  RemoveDir(ExpandConstant('{tmp}'));
  ExitProcess(Code);
end;

function InitializeSetup: Boolean;
var
  I, Code: Integer;
  Headless: Boolean;
  Arguments, Relay, HeadlessLog: String;
begin
  Result := True;
  Headless := False;
  HeadlessLog := '';
  Arguments := '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-';
  for I := 1 to ParamCount do begin
    if CompareText(Copy(ParamStr(I), 1, 5), '/LOG=') = 0 then
      HeadlessLog := Copy(ParamStr(I), 6, Length(ParamStr(I))) + '.headless.log';
    if ParamStr(I) = '--silent' then Headless := True
    else if (CompareText(Copy(ParamStr(I), 1, 5), '/SL5=') <> 0) and
      (CompareText(Copy(ParamStr(I), 1, 11), '/NOTIFYWND=') <> 0) and
      (CompareText(Copy(ParamStr(I), 1, 5), '/LOG=') <> 0) then
      Arguments := Arguments + ' ' + QuoteArgument(ParamStr(I));
  end;
  if Headless then begin
    try
    Relay := SetupPrivateFolder + '\headless-setup.exe';
    if HeadlessLog = '' then HeadlessLog := SetupPrivateFolder + '\headless.log';
    Arguments := Arguments + ' ' + QuoteArgument('/LOG=' + HeadlessLog);
    if not CopyFileW(ExpandConstant('{srcexe}'), Relay, True) then begin
      Log('Cannot retain the headless installer relay.');
      ExitRelay(12);
    end;
    if GetSHA256OfFile(Relay) <> GetSHA256OfFile(ExpandConstant('{srcexe}')) then
      ExitRelay(12);
    if not ShellExec('open', Relay, Arguments, '', SW_HIDE,
      ewWaitUntilTerminated, Code) then begin
      Log('Headless relay could not start: ' + SysErrorMessage(Code));
      ExitRelay(12);
    end;
    ExitRelay(Code);
    Result := False;
    except
      Log('Headless initialization failed: ' + GetExceptionMessage);
      ExitRelay(12);
      Result := False;
    end;
  end;
end;

function DefaultRoot: String;
begin
  Result := ExpandConstant('{localappdata}\' + DefaultRootName);
end;

function UnderDefaultRoot(const Path: String): Boolean;
begin
  Result := (CompareText(Path, DefaultRoot) = 0) or
    (CompareText(Copy(Path, 1, Length(DefaultRoot) + 1), DefaultRoot + '\') = 0);
end;

// Write one airk-setup-operation/1 file into a fresh folder below the private
// setup folder and return its path. Setup refuses anything but these
// canonical bytes.
function WriteOperation(const Kind, Locations, InventorySha, DeleteIndexes: String;
  Acknowledged, LoginStartup, Trust, Start: Boolean): String;
var
  Folder, Id, Text, TrustChoice: String;
  Created: Int64;
begin
  Id := NewOperationId;
  Folder := SetupPrivateFolder + '\airk-setup-' + Id;
  if not CreateDir(Folder) then
    RaiseException('AIRK could not create its setup operation folder ' + Folder);
  Created := UnixMillis;
  if Trust then TrustChoice := 'ask' else TrustChoice := 'skip';
  Text := '{"schema":"airk-setup-operation/1","operation_id":"' + Id + '"' +
    ',"kind":"' + Kind + '"' +
    ',"created_unix_ms":' + IntToStr(Created) +
    ',"expires_unix_ms":' + IntToStr(Created + OperationLifetimeMs) +
    ',"requested_by":"inno"' +
    ',"acknowledgements":{"predecessor_removal":' + JsonBool(Acknowledged) +
    ',"index_reset":' + JsonBool(Acknowledged) + '}' +
    ',"locations":' + Locations +
    ',"payload_inventory_sha256":"' + InventorySha + '"' +
    ',"options":{"login_startup":' + JsonBool(LoginStartup) +
    ',"trust":"' + TrustChoice + '"' +
    ',"start":' + JsonBool(Start) +
    ',"delete_indexes":"' + DeleteIndexes + '"}}' + #10;
  Result := Folder + '\operation.json';
  if not SaveStringToFile(Result, Utf8Encode(Text), False) then
    RaiseException('AIRK could not write its setup operation ' + Result);
  if not SaveStringToFile(Folder + '\operation-request.json', Utf8Encode(Text), False) then
    RaiseException('AIRK could not retain its setup recovery request.');
end;

function ReadText(const Path: String): String;
var
  Value: AnsiString;
begin
  Result := '';
  if FileExists(Path) and LoadStringFromFile(Path, Value) then
    Result := Utf8Decode(Value);
end;

{ Run airk's private setup role and return its exit code. }
function RunSetup(const Airk, Operation, Mode: String): Integer;
var
  Parameters: String;
begin
  Parameters := '__airk_role setup --operation "' + Operation + '"';
  if Mode <> '' then Parameters := Parameters + ' ' + Mode;
  if not Exec(Airk, Parameters, ExtractFileDir(Operation), SW_HIDE,
    ewWaitUntilTerminated, Result) then
    RaiseException('AIRK setup could not start: ' + SysErrorMessage(Result));
end;

function SetupFailure(const Operation: String): String;
begin
  Result := Trim(ReadText(ExtractFileDir(Operation) + '\error.txt'));
  if Result = '' then Result := 'AIRK setup failed without a message.';
end;

{ Extract the setup image and its inventory for the steps that precede
  placement. The image is copied into the private setup folder and verified
  there, so the bytes that run are the bytes checked. }
function PreflightImage(const Operation: String): String;
begin
  Result := SetupPrivateFolder + '\airk-setup.exe';
  if not FileExists(ExpandConstant('{tmp}\airk-setup.exe')) then
    ExtractTemporaryFile('airk-setup.exe');
  if not FileExists(ExpandConstant('{tmp}\setup-payload-inventory.json')) then
    ExtractTemporaryFile('setup-payload-inventory.json');
  if not FileExists(Result) then
    if not FileCopy(ExpandConstant('{tmp}\airk-setup.exe'), Result, True) then
      RaiseException('AIRK could not stage its setup image.');
  if CompareText(GetSHA256OfFile(Result), '{#AirkBinarySHA}') <> 0 then
    RaiseException('The packaged AIRK service image changed; download the installer again.');
  if not FileExists(ExtractFileDir(Operation) + '\payload-inventory.json') then
    if not FileCopy(ExpandConstant('{tmp}\setup-payload-inventory.json'),
      ExtractFileDir(Operation) + '\payload-inventory.json', True) then
    RaiseException('AIRK could not stage its payload inventory.');
  if CompareText(GetSHA256OfFile(ExtractFileDir(Operation) + '\payload-inventory.json'), '{#AirkInventorySHA}') <> 0 then
    RaiseException('The retained payload inventory changed.');
end;

// Create the default root private when a chosen location uses it and it
// does not exist yet. An existing root is left as it is; setup checks it.
procedure PrepareDefaultRoot;
begin
  if DirExists(DefaultRoot) then Exit;
  if not (UnderDefaultRoot(RemoveBackslashUnlessRoot(ExpandConstant('{app}'))) or
    UnderDefaultRoot(DataRoot) or UnderDefaultRoot(CacheRoot)) then Exit;
  if not CreateDir(DefaultRoot) then
    RaiseException('AIRK could not create ' + DefaultRoot);
  RestrictToUser(DefaultRoot);
end;

function InstallLocations: String;
begin
  PrepareDefaultRoot;
  Result := LocationsJson(RemoveBackslashUnlessRoot(ExpandConstant('{app}')),
    DataRoot, CacheRoot);
end;

{ The U13 disclosure: run setup --plan and load what it found. Returns False
  when the chosen locations cannot be used. }
function LoadPlan(var Text: String): Boolean;
var
  Operation, Json: String;
begin
  Operation := WriteOperation('install', InstallLocations, '{#AirkInventorySHA}', 'keep',
    False, False, False, False);
  if RunSetup(PreflightImage(Operation), Operation, '--plan') <> 0 then
    RaiseException(SetupFailure(Operation));
  Text := ReadText(ExtractFileDir(Operation) + '\plan.txt');
  Json := ReadText(ExtractFileDir(Operation) + '\plan.json');
  NeedsAcknowledgement := Pos('"removed":true', Json) > 0;
  Result := (Pos('"location_problems":[]', Json) > 0) and (Pos('"blockers":[]', Json) > 0);
end;

{ ---------- User PATH under an ownership receipt (reused from airk.iss) ---------- }

function UserPathEntry: String;
begin
  Result := ExpandConstant('{app}\bin');
  if (Pos(';', Result) > 0) or (Pos(#13, Result) > 0) or
     (Pos(#10, Result) > 0) then
    RaiseException('The AIRK bin directory cannot be represented in the user PATH.');
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
    Log('The AIRK bin directory was already on the user PATH; no ownership was claimed.');
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

{ Each producer gets a private nonce-scoped projection context. Copy original
  bytes only; this is not a new operation and never rewrites Core JSON. }
function PrivateProjectionOperation(const Operation: String): String;
var
  Folder, Inventory: String;
begin
  if ProjectionOperation = '' then begin
    Folder := SetupPrivateFolder + '\recovery-projection';
    if not CreateDir(Folder) then RaiseException('AIRK could not create its private recovery projection.');
    RestrictToUser(Folder);
    ProjectionOperation := Folder + '\operation-request.json';
    ProjectionOriginalSHA := GetSHA256OfFile(Operation);
    if not FileCopy(Operation, ProjectionOperation, True) then
      RaiseException('The original recovery request could not be retained for this producer.');
    if GetSHA256OfFile(ProjectionOperation) <> ProjectionOriginalSHA then
      RaiseException('The original recovery request changed while retaining its exact bytes.');
    Inventory := ExtractFileDir(Operation) + '\payload-inventory.json';
    if not FileCopy(Inventory, Folder + '\payload-inventory.json', True) then
      RaiseException('The original raw recovery inventory is missing; preserve the original recovery folder.');
    if GetSHA256OfFile(Inventory) <> GetSHA256OfFile(Folder + '\payload-inventory.json') then
      RaiseException('The original raw recovery inventory changed while copying.');
  end;
  if FileExists(Operation) and (GetSHA256OfFile(Operation) <> ProjectionOriginalSHA) then
    RaiseException('The original operation changed after recovery projection.');
  Result := ProjectionOperation;
end;

{ The native owner authenticates and projects each scalar; Pascal never decodes I4. }
function RecoveryField(const Operation, Name: String): String;
var
  Projection: String;
begin
  Projection := PrivateProjectionOperation(Operation);
  if RunSetup(PreflightImage(Operation), Projection, '--recovery-field ' + Name) <> 0 then
    RaiseException(SetupFailure(Projection));
  Result := ReadText(ExtractFileDir(Projection) + '\recovery-' + Name + '.txt');
end;

procedure AcquireWrapperLease(const Image, Operation: String);
var
  ResultCode: Cardinal;
  Projection: String;
begin
  if WrapperLease <> 0 then Exit;
  Projection := PrivateProjectionOperation(Operation);
  if RunSetup(Image, Projection, '--recovery-field wrapper-lock-handle') <> 0 then
    RaiseException(SetupFailure(Projection));
  WrapperLease := StrToInt64(ReadText(ExtractFileDir(Projection) + '\recovery-wrapper-lock-handle.txt'));
  if WrapperLease = 0 then RaiseException('Native wrapper lease handle is missing.');
  ResultCode := WaitForSingleObject(WrapperLease, 0);
  if (ResultCode <> 0) and (ResultCode <> 128) then begin
    CloseHandle(WrapperLease);
    WrapperLease := 0;
    RaiseException('Another AIRK installer is still producing files. Preserve the original request and retry after it exits.');
  end;
end;

procedure ReleaseWrapperLease;
begin
  if WrapperLease <> 0 then begin
    ReleaseMutex(WrapperLease);
    CloseHandle(WrapperLease);
    WrapperLease := 0;
  end;
end;

procedure DeinitializeSetup;
begin
  ReleaseWrapperLease;
end;

procedure DeinitializeUninstall;
begin
  ReleaseWrapperLease;
end;

function ShouldPlacePayload: Boolean;
begin
  Result := (RecoveryOperation = '') or RecoveryPlacement;
end;

procedure LoadRecovery;
var
  I: Integer;
  Arg, Kind, Data, Cache, Value: String;
begin
  RecoveryOperation := ExpandConstant('{param:AIRKOPERATION|}');
  if RecoveryOperation = '' then Exit;
  for I := 1 to ParamCount do begin
    Arg := Uppercase(ParamStr(I));
    if (Pos('/DIR=', Arg) = 1) or (Pos('/DATA=', Arg) = 1) or
       (Pos('/CACHE=', Arg) = 1) or (Pos('/TRUST', Arg) = 1) or
       (Pos('/ACKNOWLEDGE-RESET', Arg) = 1) then
      RaiseException('/AIRKOPERATION resumes original locations and options; overrides are forbidden.');
  end;
  if (not FileExists(RecoveryOperation)) or (not FileExists(ExtractFileDir(RecoveryOperation) + '\payload-inventory.json')) then
    RaiseException('Preserve the complete original private recovery folder and request.');
  if RecoveryField(RecoveryOperation, 'requested-by') <> 'inno' then
    RaiseException('The original operation belongs to a different installer.');
  Kind := RecoveryField(RecoveryOperation, 'kind');
  if (Kind <> 'install') and (Kind <> 'repair') and (Kind <> 'upgrade') then
    RaiseException('Removal must resume through the original uninstaller.');
  if RecoveryField(RecoveryOperation, 'inventory-sha256') <> '{#AirkInventorySHA}' then
    RaiseException('Recovery requires the exact original installer payload.');
  WizardForm.DirEdit.Text := RecoveryField(RecoveryOperation, 'program');
  Data := RecoveryField(RecoveryOperation, 'data');
  Cache := RecoveryField(RecoveryOperation, 'cache');
  DataPage.Values[0] := ExtractFileDir(ExtractFileDir(Data));
  DataPage.Values[1] := ExtractFileDir(ExtractFileDir(Cache));
  if (Data <> DataRoot + '\data\indexes') or (Cache <> CacheRoot + '\cache\service') or
     (RecoveryField(RecoveryOperation, 'state') <> DataRoot + '\state\service') or
     (RecoveryField(RecoveryOperation, 'logs') <> CacheRoot + '\logs\service') then
    RaiseException('The original locations do not match this installer layout.');
  Value := RecoveryField(RecoveryOperation, 'placement-required');
  if (Value <> 'true') and (Value <> 'false') then RaiseException('Invalid native recovery observation.');
  RecoveryPlacement := Value = 'true';
  Value := RecoveryField(RecoveryOperation, 'completion-required');
  if (Value <> 'true') and (Value <> 'false') then RaiseException('Invalid native completion observation.');
  RecoveryCompletion := Value = 'true';
  if RecoveryPlacement and (not RecoveryCompletion) then RaiseException('Native placement and completion observations conflict.');
  if RecoveryCompletion and WizardSilent then
    if RecoveryField(RecoveryOperation, 'trust') <> 'skip' then
      RaiseException('The original request requires consent; resume interactively without --silent.');
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := (RecoveryOperation <> '') and
    ((PageID = wpSelectDir) or (PageID = DataPage.ID) or (PageID = DisclosurePage.ID));
end;

{ ---------- Wizard: data locations and the U13 disclosure ---------- }

procedure AcknowledgeClicked(Sender: TObject);
begin
  WizardForm.NextButton.Enabled := (not NeedsAcknowledgement) or AcknowledgeBox.Checked;
end;

procedure InitializeWizard;
begin
  DataPage := CreateInputDirPage(wpSelectDir, 'AIRK data locations',
    'Where should AIRK keep its indexes, service state, cache and logs?',
    'AIRK keeps index data and service state in one folder, and cache and logs in another. ' +
    'Choose folders that only your account uses. Index data moves later only through the ' +
    'AIRK admin Storage page or airkctl, never by this installer.' + #13#10 + #13#10 +
    'If VS Code is open, update the AIRK extension to {#AirkVersion} or later first, or close ' +
    'VS Code. With an up-to-date extension, open VS Code windows release AIRK while setup ' +
    'runs and reconnect when it finishes; an older extension keeps AIRK in use or sets up ' +
    'the old AIRK again.',
    False, '');
  DataPage.Add('Index data and service state');
  DataPage.Add('Cache and logs');
  DataPage.Values[0] := ExpandConstant('{param:DATA|' +
    GetPreviousData('DataRoot', DefaultRoot) + '}');
  DataPage.Values[1] := ExpandConstant('{param:CACHE|' +
    GetPreviousData('CacheRoot', DefaultRoot) + '}');

  DisclosurePage := CreateCustomPage(DataPage.ID, 'Earlier AIRK installations',
    'Review what this installation changes before anything is changed.');
  DisclosureMemo := TNewMemo.Create(DisclosurePage);
  DisclosureMemo.Parent := DisclosurePage.Surface;
  DisclosureMemo.ReadOnly := True;
  DisclosureMemo.ScrollBars := ssVertical;
  DisclosureMemo.WordWrap := True;
  DisclosureMemo.SetBounds(0, 0, DisclosurePage.SurfaceWidth,
    DisclosurePage.SurfaceHeight - ScaleY(40));
  AcknowledgeBox := TNewCheckBox.Create(DisclosurePage);
  AcknowledgeBox.Parent := DisclosurePage.Surface;
  AcknowledgeBox.Caption := 'I understand that earlier AIRK installations and their indexes will be removed.';
  AcknowledgeBox.SetBounds(0, DisclosurePage.SurfaceHeight - ScaleY(30),
    DisclosurePage.SurfaceWidth, ScaleY(24));
  AcknowledgeBox.OnClick := @AcknowledgeClicked;

  PlanProgressPage := CreateOutputMarqueeProgressPage('Checking AIRK data locations',
    'Setup is checking the chosen folders and looking for earlier AIRK installations.');
  LoadRecovery;
end;

procedure RegisterPreviousData(PreviousDataKey: Integer);
begin
  SetPreviousData(PreviousDataKey, 'DataRoot', DataRoot);
  SetPreviousData(PreviousDataKey, 'CacheRoot', CacheRoot);
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Text: String;
begin
  Result := True;
  if CurPageID = DataPage.ID then begin
    try
      { Exec waits while processing window messages, so the marquee keeps
        moving during setup --plan. Hide runs before any message box. }
      PlanProgressPage.SetText('This can take a minute. Setup has not stopped.', '');
      PlanProgressPage.Show;
      try
        PlanProgressPage.Animate;
        Result := LoadPlan(Text);
      finally
        PlanProgressPage.Hide;
      end;
      if not Result then begin
        MsgBox(Text + #13#10 + #13#10 + 'Resolve the items above (choose other folders on this page, or update the AIRK extension or close VS Code), then click Next.',
          mbError, MB_OK);
        Result := False;
        Exit;
      end;
      DisclosureMemo.Text := Text;
      AcknowledgeBox.Checked := False;
      AcknowledgeBox.Visible := NeedsAcknowledgement;
    except
      MsgBox(GetExceptionMessage, mbError, MB_OK);
      Result := False;
    end;
  end;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = DisclosurePage.ID then
    AcknowledgeClicked(nil);
  if (CurPageID = wpFinished) and FinalizationFailed then begin
    WizardForm.FinishedHeadingLabel.Caption := 'AIRK setup needs recovery';
    WizardForm.FinishedLabel.Caption := FinalizationError;
  end;
end;

{ Unattended installs take the same choices as arguments (section 9.1):
  /DIR=, /DATA=, /CACHE=, and /ACKNOWLEDGE-RESET when an earlier AIRK
  installation will be removed; /TRUST asks for certificate trust (an
  unattended install skips it by default because the OS asks for consent). }
function Acknowledged: Boolean;
begin
  if WizardSilent then
    Result := ExpandConstant('{param:ACKNOWLEDGE-RESET|0}') <> '0'
  else
    Result := (not NeedsAcknowledgement) or AcknowledgeBox.Checked;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Text: String;
  Trust: Boolean;
begin
  Result := '';
  try
    if RecoveryOperation <> '' then begin
      SetupOperation := RecoveryOperation;
      AcquireWrapperLease(PreflightImage(SetupOperation), SetupOperation);
      if RecoveryPlacement then
        if RunSetup(PreflightImage(SetupOperation), SetupOperation, '--prepare') <> 0 then
          Result := SetupFailure(SetupOperation);
      Exit;
    end;
    if WizardSilent then begin
      if not LoadPlan(Text) then begin
        Result := Text;
        Exit;
      end;
      if NeedsAcknowledgement and not Acknowledged then begin
        Result := Text + #13#10 + 'Pass /ACKNOWLEDGE-RESET to confirm this unattended install.';
        Exit;
      end;
      Trust := False;
      if ExpandConstant('{param:TRUST|0}') <> '0' then begin
        Result := 'Certificate trust needs consent. Run interactive setup or omit /TRUST.';
        Exit;
      end;
    end else
      Trust := True;
    SetupOperation := WriteOperation('install', InstallLocations, '{#AirkInventorySHA}', 'keep',
      Acknowledged, True, Trust, True);
    AcquireWrapperLease(PreflightImage(SetupOperation), SetupOperation);
    { Retirement and the drain run before any file is replaced (section 9.6). }
    if RunSetup(PreflightImage(SetupOperation), SetupOperation, '--prepare') <> 0 then
      Result := SetupFailure(SetupOperation);
  except
    Result := GetExceptionMessage;
  end;
end;

function InstalledBinary(const Relative, Expected: String): String;
begin
  Result := ExpandConstant('{app}\' + Relative);
  if (not FileExists(Result)) or (CompareText(GetSHA256OfFile(Result), Expected) <> 0) then
    RaiseException('The installed ' + Relative + ' changed; its files were preserved.');
end;

function UninstallSourceImage: String;
begin
  Result := '';
  if ExpandConstant('{param:AIRKOPERATION|}') <> '' then
    Result := ExpandConstant('{param:AIRKRECOVERYIMAGE|}');
  if Result = '' then Result := InstalledBinary('service\airk.exe', '{#AirkBinarySHA}');
  if (not FileExists(Result)) or (CompareText(GetSHA256OfFile(Result), '{#AirkBinarySHA}') <> 0) then
    RaiseException('The exact verified retained removal image is missing or changed; preserve the original recovery folder.');
end;

{ Uninstall cannot extract setup-only resources. Retain the installed verified
  image outside the tracked program before native removal begins. }
function RetainedInstalledImage(const Operation: String): String;
var
  Source, Inventory, SourceInventory: String;
begin
  Source := UninstallSourceImage;
  Result := SetupPrivateFolder + '\airk-setup.exe';
  if not FileCopy(Source, Result, True) then
    RaiseException('AIRK could not retain its verified uninstall image.');
  if CompareText(GetSHA256OfFile(Result), '{#AirkBinarySHA}') <> 0 then
    RaiseException('The retained uninstall image changed.');
  Inventory := ExtractFileDir(Operation) + '\payload-inventory.json';
  if not FileExists(Inventory) then begin
    SourceInventory := ExpandConstant('{app}\payload-inventory.json');
    if (not FileExists(SourceInventory)) or
      (CompareText(GetSHA256OfFile(SourceInventory), '{#AirkInventorySHA}') <> 0) then
      RaiseException('The original uninstall inventory is missing or changed; preserve the original recovery folder.');
    if not FileCopy(SourceInventory, Inventory, True) then
      RaiseException('AIRK could not retain its original uninstall inventory.');
  end;
  if CompareText(GetSHA256OfFile(Inventory), '{#AirkInventorySHA}') <> 0 then
    RaiseException('The retained original uninstall inventory changed.');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then begin
    try
      InstalledBinary('bin\airkctl.exe', '{#AirkCtlBinarySHA}');
      { The same operation and retained image complete after placed-image checks. }
      InstalledBinary('service\airk.exe', '{#AirkBinarySHA}');
      if ((RecoveryOperation = '') or RecoveryCompletion) and
        (RunSetup(PreflightImage(SetupOperation), SetupOperation, '') <> 0) then
        RaiseException('AIRK Service files are installed, but setup did not complete:' + #13#10 +
          SetupFailure(SetupOperation));
      AddOwnedUserPath;
      { Inno has finalized and closed both uninstaller files before ssPostInstall. }
      if RunSetup(PreflightImage(SetupOperation),
        ExtractFileDir(SetupOperation) + '\operation-request.json', '--seal-inno') <> 0 then
        RaiseException('AIRK Service is installed, but its verified removal handoff could not be retained:' + #13#10 +
          SetupFailure(SetupOperation));
      if not SaveStringToFile(SetupPrivateFolder + '\wrapper-outcome.txt',
        Utf8Encode('complete' + #10 + 'operation_sha256=' + GetSHA256OfFile(ExtractFileDir(SetupOperation) + '\operation-request.json') + #10), False) then
        RaiseException('AIRK could not retain its wrapper completion diagnostic.');
      { Earlier installations setup could not remove are still installed. }
      if FileExists(ExtractFileDir(SetupOperation) + '\leftovers.txt') and (not WizardSilent) then
        MsgBox(Trim(ReadText(ExtractFileDir(SetupOperation) + '\leftovers.txt')) + #13#10 + #13#10 +
          'Details are in the setup receipt; "airkctl service status" shows the new service.',
          mbInformation, MB_OK);
    except
      FinalizationFailed := True;
      FinalizationError := GetExceptionMessage + #13#10 + #13#10 +
        'Recovery request and verified setup image: ' + SetupPrivateFolder + #13#10 +
        'Resume this exact installer with --silent /AIRKOPERATION="' + ExtractFileDir(SetupOperation) + '\operation-request.json". Do not add location or option overrides.';
      SaveStringToFile(SetupPrivateFolder + '\wrapper-outcome.txt',
        Utf8Encode('failed' + #10 + FinalizationError + #10), False);
      Log(FinalizationError);
      if not WizardSilent then MsgBox(FinalizationError, mbError, MB_OK);
    end;
  end;
end;

function GetCustomSetupExitCode: Integer;
begin
  if FinalizationFailed then Result := 11 else Result := 0;
end;

{ ---------- Uninstall: the setup role removes, Inno removes its files ---------- }

function InitializeUninstall: Boolean;
var
  Verified: String;
begin
  Result := False;
  try
    { This event precedes confirmation. Validate read-only; Cancel changes nothing. }
    Verified := UninstallSourceImage;
    Result := True;
  except
    Log(GetExceptionMessage);
    if not UninstallSilent then MsgBox(GetExceptionMessage, mbError, MB_OK);
  end;
end;

function PreviousRoot(const Name: String): String;
begin
  if not RegQueryStringValue(HKCU, AppKey, 'Inno Setup CodeFile: ' + Name, Result) then
    Result := DefaultRoot;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Operation, DeleteIndexes, Image, CompletionRequired, Projection: String;
  Handoff: Boolean;
begin
  if CurUninstallStep = usUninstall then begin
    try
      Operation := ExpandConstant('{param:AIRKOPERATION|}');
      Handoff := Operation <> '';
      DeleteIndexes := 'keep';
      if (Operation = '') and (not UninstallSilent) and (MsgBox('Also delete the AIRK index data, service state, cache and logs?' + #13#10#13#10 +
        'Choose No to keep them for a later AIRK Service installation. Source files are never touched.',
        mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
        DeleteIndexes := 'purge';
      if Operation = '' then Operation := WriteOperation('uninstall',
        LocationsJson(RemoveBackslashUnlessRoot(ExpandConstant('{app}')),
          PreviousRoot('DataRoot'), PreviousRoot('CacheRoot')),
        '{#AirkInventorySHA}', DeleteIndexes, False, False, False, False);
      if not FileExists(Operation) then RaiseException('The original uninstall operation is missing.');
      UninstallOperationSHA := GetSHA256OfFile(Operation);
      Image := RetainedInstalledImage(Operation);
      AcquireWrapperLease(Image, Operation);
      CompletionRequired := 'true';
      if Handoff then begin
        Projection := PrivateProjectionOperation(Operation);
        if RunSetup(Image, Projection, '--recovery-field completion-required') <> 0 then
          RaiseException(SetupFailure(Projection));
        CompletionRequired := ReadText(ExtractFileDir(Projection) + '\recovery-completion-required.txt');
      end;
      if (CompletionRequired <> 'true') and (CompletionRequired <> 'false') then
        RaiseException('Invalid native removal recovery observation.');
      if (CompletionRequired = 'true') and (RunSetup(Image, Operation, '') <> 0) then
        RaiseException('AIRK Service uninstall did not complete; its files were preserved:' + #13#10 +
          SetupFailure(Operation));
      RemoveOwnedUserPath;
      SaveStringToFile(SetupPrivateFolder + '\wrapper-outcome.txt',
        Utf8Encode('native-complete-wrapper-removal-pending' + #10 + 'operation_sha256=' + UninstallOperationSHA + #10), False);
    except
      SaveStringToFile(SetupPrivateFolder + '\wrapper-outcome.txt', Utf8Encode('failed' + #10 + GetExceptionMessage + #10 +
        'Resume the same original uninstaller with /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /AIRKOPERATION="' + Operation +
        '" /AIRKRECOVERYIMAGE="' + Image + '". Preserve its matching EXE and DAT.' + #10), False);
      Log(GetExceptionMessage);
      if not UninstallSilent then MsgBox(GetExceptionMessage, mbError, MB_OK);
      { Abort stops Inno's tracked-file removal (pinned Inno 6.7.1). }
      Abort;
    end;
  end;
  if CurUninstallStep = usDone then
    if not SaveStringToFile(SetupPrivateFolder + '\wrapper-outcome.txt',
      Utf8Encode('complete' + #10 + 'operation_sha256=' + UninstallOperationSHA + #10), False) then
      Log('Uninstall finished, but its wrapper completion diagnostic could not be written.');
end;
