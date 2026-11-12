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
Source: "{#AirkPayload}\service\airk.exe"; DestDir: "{app}\service"; Flags: ignoreversion
Source: "{#AirkPayload}\bin\airkctl.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#AirkPayload}\notices\*"; DestDir: "{app}\notices"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#AirkPayload}\payload-inventory.json"; DestDir: "{app}"; Flags: ignoreversion
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

{ JSON string exactly as serde_json writes it for printable ASCII; anything
  else is refused, so the operation stays canonical. }
function JsonString(Value: String): String;
var
  I: Integer;
begin
  for I := 1 to Length(Value) do
    if (Ord(Value[I]) < 32) or (Ord(Value[I]) > 126) then
      RaiseException('AIRK locations must use printable ASCII characters: ' + Value);
  StringChangeEx(Value, '\', '\\', True);
  StringChangeEx(Value, '"', '\"', True);
  Result := '"' + Value + '"';
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
    PrivateFolder := ExpandConstant('{tmp}\airk-private');
    if not CreateDir(PrivateFolder) then
      RaiseException('AIRK could not create its private setup folder ' + PrivateFolder);
    RestrictToUser(PrivateFolder);
  end;
  Result := PrivateFolder;
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
  if not SaveStringToFile(Result, Text, False) then
    RaiseException('AIRK could not write its setup operation ' + Result);
end;

function ReadText(const Path: String): String;
var
  Value: AnsiString;
begin
  Result := '';
  if FileExists(Path) and LoadStringFromFile(Path, Value) then
    Result := String(Value);
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
  if not FileCopy(ExpandConstant('{tmp}\setup-payload-inventory.json'),
    ExtractFileDir(Operation) + '\payload-inventory.json', False) then
    RaiseException('AIRK could not stage its payload inventory.');
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
    'Before you continue, update the AIRK extension in VS Code to {#AirkVersion} or later, or close ' +
    'VS Code. An older AIRK extension sets up the old AIRK again.',
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
        MsgBox(Text + #13#10 + #13#10 + 'Resolve the items above (choose other folders on this page, or update or close VS Code), then click Next.',
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
    if WizardSilent then begin
      if not LoadPlan(Text) then begin
        Result := Text;
        Exit;
      end;
      if NeedsAcknowledgement and not Acknowledged then begin
        Result := Text + #13#10 + 'Pass /ACKNOWLEDGE-RESET to confirm this unattended install.';
        Exit;
      end;
      Trust := ExpandConstant('{param:TRUST|0}') <> '0';
    end else
      Trust := True;
    SetupOperation := WriteOperation('install', InstallLocations, '{#AirkInventorySHA}', 'keep',
      Acknowledged, True, Trust, True);
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

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then begin
    try
      InstalledBinary('bin\airkctl.exe', '{#AirkCtlBinarySHA}');
      { The same operation resumes from its receipt with the placed image. }
      if RunSetup(InstalledBinary('service\airk.exe', '{#AirkBinarySHA}'),
        SetupOperation, '') <> 0 then
        RaiseException('AIRK Service files are installed, but setup did not complete:' + #13#10 +
          SetupFailure(SetupOperation));
      AddOwnedUserPath;
      { Earlier installations setup could not remove are still installed. }
      if FileExists(ExtractFileDir(SetupOperation) + '\leftovers.txt') then
        MsgBox(Trim(ReadText(ExtractFileDir(SetupOperation) + '\leftovers.txt')) + #13#10 + #13#10 +
          'Details are in the setup receipt; "airkctl service status" shows the new service.',
          mbInformation, MB_OK);
    except
      MsgBox(GetExceptionMessage + #13#10 + #13#10 +
        'Run "airkctl service status" for details; running this installer again repairs the installation.',
        mbError, MB_OK);
    end;
  end;
end;

{ ---------- Uninstall: the setup role removes, Inno removes its files ---------- }

function InitializeUninstall: Boolean;
var
  Verified: String;
begin
  Result := False;
  try
    { This event precedes confirmation. Validate read-only; Cancel changes nothing. }
    Verified := InstalledBinary('service\airk.exe', '{#AirkBinarySHA}');
    Result := True;
  except
    MsgBox(GetExceptionMessage, mbError, MB_OK);
  end;
end;

function PreviousRoot(const Name: String): String;
begin
  if not RegQueryStringValue(HKCU, AppKey, 'Inno Setup CodeFile: ' + Name, Result) then
    Result := DefaultRoot;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Operation, DeleteIndexes: String;
begin
  if CurUninstallStep = usUninstall then begin
    try
      DeleteIndexes := 'keep';
      if (not UninstallSilent) and (MsgBox('Also delete the AIRK index data, service state, cache and logs?' + #13#10#13#10 +
        'Choose No to keep them for a later AIRK Service installation. Source files are never touched.',
        mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
        DeleteIndexes := 'purge';
      Operation := WriteOperation('uninstall',
        LocationsJson(RemoveBackslashUnlessRoot(ExpandConstant('{app}')),
          PreviousRoot('DataRoot'), PreviousRoot('CacheRoot')),
        '{#AirkInventorySHA}', DeleteIndexes, False, False, False, False);
      if RunSetup(InstalledBinary('service\airk.exe', '{#AirkBinarySHA}'), Operation, '') <> 0 then
        RaiseException('AIRK Service uninstall did not complete; its files were preserved:' + #13#10 +
          SetupFailure(Operation));
      RemoveOwnedUserPath;
    except
      MsgBox(GetExceptionMessage, mbError, MB_OK);
      { Abort stops Inno's tracked-file removal (pinned Inno 6.7.1). }
      Abort;
    end;
  end;
end;
