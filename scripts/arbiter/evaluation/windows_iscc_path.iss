; SPDX-License-Identifier: Apache-2.0
; Compiler-only source-path control. The resulting executable is never run.
[Setup]
AppName=AIRK compiler path control
AppVersion=1
DefaultDirName={localappdata}\AIRK compiler path control
PrivilegesRequired=lowest
Uninstallable=no
OutputDir={#AirkOutput}
OutputBaseFilename=compiler-control
Compression=none
[Files]
Source: "{#AirkSource}"; DestDir: "{app}"; Flags: dontcopy
