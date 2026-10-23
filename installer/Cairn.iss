; Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
; GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
; content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
; Kingdom Come: Together -- one-click installer (the new name; before, "KCD2 Multiplayer").
;
; Compile with tools\Build-Installer.ps1, not by hand: this script installs
; the *output* of tools\Publish-Release.ps1 (release\Cairn\) and will refuse
; to compile if that folder is not there.
;
; What it does that a plain file-copy installer does not:
;   * knows nothing of any game: it installs the launcher, and the launcher is
;     where a game is chosen, its mod downloaded and its setup done (each game's
;     pack does that, see docs\PACKS.md),
;   * verifies after installing that every file landed and nothing foreign sits
;     in the install folder,
;   * installs the WebView2 runtime when missing, which the Photino-based
;     launcher cannot render without.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

; the name players see. The colon cannot be in a file name, so shortcuts and
; the Start menu folder use ShortcutName. Never renamed (installs, saves and upgrades
; depend on them): the AppId below, the install folder {localappdata}\Cairn, the
; HKCU\Software\Cairn key, the cairn mod folder, the launcher's exe name.
#define AppName "Cairn Client"
#define ShortcutName "Cairn"
#define OldShortcutName "KCD2 Multiplayer"
#define Old2ShortcutName "Kingdom Come Together"
#define Old3ShortcutName "Cairn"
#ifndef CairnVersion
  #define CairnVersion AppVersion
#endif
#define Disclaimer "Unofficial. Not affiliated with or endorsed by the studios or publishers of the games it supports."
#define AppExeName "Cairn.Launcher.exe"
#define AppPublisher "Cairn"
#define AppUrl "https://github.com/Cairn-client/Launcher"


; Evergreen WebView2 Runtime bootstrapper. Microsoft's documented permanent
; redirect; ~2 MB, and it installs per-user when Setup is not elevated, which
; is exactly our case (PrivilegesRequired=lowest).
#define WebView2BootstrapUrl "https://go.microsoft.com/fwlink/p/?LinkId=2124703"

[Setup]
AppId={{88C5B9F1-0E71-4D60-9418-5575D5684F95}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#CairnVersion}
VersionInfoVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}
AppUpdatesURL={#AppUrl}

; %LocalAppData%, deliberately, and PrivilegesRequired=lowest with it:
;   * the launcher writes settings.json in its own working directory
;     (Cairn.Launcher/Pages/Home.razor.cs:33), so a Program Files install
;     would need elevation every time the user pressed Save in Settings;
;   * the mod's target folder is inside the Steam library, and Steam grants
;     BUILTIN\Users FullControl on its own tree, so an unelevated install can
;     write <ModdingTools>\Mods even when Steam sits in Program Files;
;   * a non-technical friend never sees a UAC prompt.
DefaultDirName={localappdata}\Cairn
PrivilegesRequired=lowest
UsePreviousAppDir=yes

DefaultGroupName={#ShortcutName}
; an upgrade must use the NEW folder name, not the one the old install chose.
UsePreviousGroup=no
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
OutputDir=..\release
OutputBaseFilename=Cairn-Setup-{#AppVersion}
; the installer's own icon (the launcher's multi-size .ico, built by tools\Build-Branding.py)
SetupIconFile=..\Cairn.Launcher\app.ico
; the exe keeps its path across the rename, so Windows' icon cache kept showing the
; old logo on the upgraded shortcuts (maintainer's report). The shortcuts name app.ico
; directly (a path the cache has not seen), and Setup tells Explorer to refresh icons at
; the end (ChangesAssociations = SHChangeNotify(SHCNE_ASSOCCHANGED); nothing is registered).
ChangesAssociations=yes
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Setup appearance: "dark" (forces dark mode) or "dynamic" (matches Windows light/dark system setting).
; Inno Setup 6.6+ supports dark mode natively with styled immersive title bars ('includetitlebar').
#ifndef WizardAppearance
  #define WizardAppearance "dark"
#endif

WizardStyle=modern {#WizardAppearance} includetitlebar
WizardImageStretch=yes

#if WizardAppearance == "dark"
  WizardImageFile=wizard-image-dark.png
  WizardSmallImageFile=wizard-small.png
#else
  WizardImageFile=wizard-image-light.png
  WizardImageFileDynamicDark=wizard-image-dark.png
  WizardSmallImageFile=wizard-small.png
  WizardSmallImageFileDynamicDark=wizard-small.png
#endif

UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExeName}
SetupLogging=yes
CloseApplications=yes
; signed only when tools\Build-Installer.ps1 found the signing settings (it defines the tool).
#ifdef cairnSign
SignTool=cairnsign
SignedUninstaller=yes
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Messages]
; the disclaimer on every renamed surface.
WelcomeLabel2=This will install [name/ver] on your computer.%n%n{#Disclaimer}%n%nIt is recommended that you close the game, the launcher and any other applications before continuing.
FinishedLabel=Setup has finished installing [name] on your computer. The application may be launched by selecting the installed shortcuts.%n%n{#Disclaimer}

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
; The whole Publish-Release.ps1 output: launcher, agent, relay, native DLL,
; injector, and the self-contained .NET runtime beside them. The launcher
; resolves relative DllPath/AgentPath/RelayPath against its own directory,
; so this flat layout is the one AppSettings' defaults already expect.
;
; overwritereadonly is not decoration. Without it, ONE read-only file
; anywhere in the install directory stops the entire install dead: Inno raises
; "The existing file could not be replaced because it is marked read-only" as
; an Abort/Retry/Ignore box, /SUPPRESSMSGBOXES answers Abort, and Setup exits
; 5 -- the same code as a user pressing Cancel. Reproduced on 2026-08-28
; against Setup 0.18.8: the abort landed on appsettings.json, the third file
; alphabetically, so every file after it kept its old build, and the mod
; folder had already been emptied by PrepareToInstall. A read-only attribute
; is not exotic: restores from backup, copies off read-only media, and some
; antivirus quarantine-restores all set it.
Source: "..\release\Cairn\*"; DestDir: "{app}"; Flags: ignoreversion overwritereadonly recursesubdirs createallsubdirs

; The game mod, and ONLY these two files. Do not turn this back into a
; wildcard over cairn\.
;
; cairn\Data\ also holds the pak's *sources* -- Libs\Tables\...,
; Scripts\Startup\cairn.lua -- which tools\Build-And-Install-
; Mod.ps1 packs into cairn.pak and deliberately does not copy. Deploying them
; loose as well breaks the game outright: a loose Data\Libs\Tables directory
; inside a mod takes over the engine's table root, and every base table then
; fails to resolve. The game dies at startup with "114 tables are not loaded",
; and nothing in that message points at this. Observed for real on 2026-07-30.
;
; uninsneveruninstall is deliberate and load-bearing too: without it the
; uninstaller silently deletes these files out of the player's game folder,
; which is not ours to do unasked. Removal is handled by the explicit
; question in CurUninstallStepChanged instead.
;
; This installer no longer carries the mod at all. Each game's mod is a separate
; pack, published with the release (tools\Build-Packs.ps1) and downloaded by the
; launcher when the player chooses it; the launcher then places it once the
; Modding Tools are set up. So there is nothing for this script to copy into a
; game folder.


[Icons]
; WorkingDir matters and is not decoration: settings.json is a bare relative
; filename in the launcher, so it lands wherever the process was started from.
Name: "{group}\{#ShortcutName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\app.ico"; Comment: "{#Disclaimer}"
Name: "{group}\Uninstall {#ShortcutName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#ShortcutName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\app.ico"; Tasks: desktopicon; Check: ShouldMakeDesktopIcon; Comment: "{#Disclaimer}"

[InstallDelete]
; an upgrade from a build before the rename replaces its shortcuts, so the
; Start menu and the desktop never show both names. Only the old shortcut files and
; the old folder once it is empty -- nothing else there is ours.
Type: files; Name: "{autoprograms}\{#OldShortcutName}\{#OldShortcutName}.lnk"
Type: files; Name: "{autoprograms}\{#OldShortcutName}\Uninstall {#OldShortcutName}.lnk"
Type: dirifempty; Name: "{autoprograms}\{#OldShortcutName}"
Type: files; Name: "{autodesktop}\{#OldShortcutName}.lnk"
; Cairn: the shortcuts of the name before Cairn (Kingdom Come Together)
Type: files; Name: "{autoprograms}\{#Old2ShortcutName}\{#Old2ShortcutName}.lnk"
Type: files; Name: "{autoprograms}\{#Old2ShortcutName}\Uninstall {#Old2ShortcutName}.lnk"
Type: dirifempty; Name: "{autoprograms}\{#Old2ShortcutName}"
Type: files; Name: "{autodesktop}\{#Old2ShortcutName}.lnk"
; Cairn: and the shortcuts of the name before Cairn (Cairn)
Type: files; Name: "{autoprograms}\{#Old3ShortcutName}\{#Old3ShortcutName}.lnk"
Type: files; Name: "{autoprograms}\{#Old3ShortcutName}\Uninstall {#Old3ShortcutName}.lnk"
Type: dirifempty; Name: "{autoprograms}\{#Old3ShortcutName}"
Type: files; Name: "{autodesktop}\{#Old3ShortcutName}.lnk"

[Registry]
; Not settings -- just enough for the uninstaller to find the mod it deployed
; into someone else's game folder, and for a later Setup to recognise its own
; earlier deployment instead of treating it as a foreign cairn.
; Listed first so it is *undone* last (uninstall replays this section in
; reverse): uninsdeletekeyifempty on a later line ran while the other values
; were still present and left an orphan key behind.
Root: HKCU; Subkey: "Software\Cairn"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Cairn"; ValueType: string; ValueName: "InstallDir"; ValueData: "{app}"


[Run]
Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; Description: "Launch {#AppName} now"; Flags: nowait postinstall skipifsilent
; Cairn: the launcher's one-click update runs this Setup silently with /RELAUNCH=1 and wants the new launcher back
Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; Flags: nowait; Check: RelaunchAfterInstall

[UninstallDelete]
; settings.json, favourites and the native plugin's log are created at
; runtime, not by [Files], so nothing else would clean them up.
Type: filesandordirs; Name: "{app}"

[Code]
var
  // post-install verification state. See VerifyInstalledFiles.
  // (Line comments, not brace ones: an Inno constant in braces inside a brace
  // comment closes the comment early -- Pascal comments do not nest.)
  ManApp: TArrayOfString;      // 'rel|size|sha256' per file in the install dir
  ManMod: TArrayOfString;      // same, for the two files in the game folder
  AppIndex: String;            // '|rel|rel|...|', lowercased -- the sweep's lookup
  VerifyFailed: Boolean;

{ The only way to hand a caller a non-zero exit code from a Setup that
  otherwise "succeeded". Inno finishes normally after ssPostInstall and has no
  API for this, and the alternative -- raising an exception in ssPostInstall --
  triggers a rollback that uninstalls the files we just verified as good.
  Cost of doing it this way: Setup's own temp folder is not cleaned up on
  this path. That is a leaked temp folder on an install that already failed, in
  exchange for automation being able to see that it failed at all. }
procedure ExitProcess(uExitCode: UINT); external 'ExitProcess@kernel32.dll stdcall';

{ Pascal Script has no attribute API of its own. Clearing read-only before a
  delete is the same defence overwritereadonly gives the file copy -- see the
  Files-section comment for what one read-only file did to a whole install. }
const
  FILE_ATTR_NORMAL = $00000080;

function SetFileAttributesW(lpFileName: String; dwFileAttributes: LongInt): Boolean;
  external 'SetFileAttributesW@kernel32.dll stdcall';

procedure ClearReadOnly(const Path: String);
begin
  SetFileAttributesW(Path, FILE_ATTR_NORMAL);
end;

{ ------------------------------------------------- Cairn: safety copy of the player's data

  Before anything is installed, a copy of what the player would hate to lose goes to
  Documents\Cairn-backup\<date-time>: the game's saves folder, and the mod's own data in
  %LOCALAPPDATA%\Cairn (henry, chests, start-save, hosted.json, ledger.json, host.key).
  After the install:
    1. a file that has gone missing from the original place is put back (robocopy with /XC /XN /XO
       copies ONLY files that are missing: nothing that exists is ever overwritten);
    2. the originals are compared with the copy (a robocopy listing run): when every one is present
       and identical, the copy is deleted; when anything differs the copy stays and the player is told where.
  The installer never deletes or edits an original; it deletes only the copy it made itself.
  robocopy ships with Windows. }

const
  BackupSetCount = 5;

var
  BackupRoot: String;
  BackupHas: array[0..4] of Boolean;
  BackupMade: Boolean;

function BackupSetName(I: Integer): String;
begin
  case I of
    0: Result := 'saves';
    1: Result := 'henry';
    2: Result := 'chests';
    3: Result := 'start-save';
  else
    Result := 'files';
  end;
end;

function BackupSetSrc(I: Integer): String;
begin
  case I of
    0: Result := GetEnv('USERPROFILE') + '\Saved Games\kingdomcome2\saves';
    1: Result := ExpandConstant('{localappdata}') + '\Cairn\henry';
    2: Result := ExpandConstant('{localappdata}') + '\Cairn\chests';
    3: Result := ExpandConstant('{localappdata}') + '\Cairn\start-save';
  else
    Result := ExpandConstant('{localappdata}') + '\Cairn';   // the three small files live here
  end;
end;

function BackupSetExists(I: Integer): Boolean;
begin
  if I < 4 then
    Result := DirExists(BackupSetSrc(I))
  else
    Result := FileExists(BackupSetSrc(4) + '\hosted.json') or FileExists(BackupSetSrc(4) + '\ledger.json')
              or FileExists(BackupSetSrc(4) + '\host.key');
end;

{ Mode 0: original -> copy. Mode 1: copy -> original, only what is missing there. Mode 2: a listing
  run copy -> original: exit code 0 means nothing is missing or different. Returns robocopy's exit
  code, or -1 when it could not be started. 0-7 is success for modes 0 and 1. }
function BackupRun(Mode, I: Integer): Integer;
var
  Src, Dst, Names, Opt, Params: String;
  ResultCode: Integer;
begin
  Src := BackupSetSrc(I);
  Dst := BackupRoot + '\' + BackupSetName(I);
  if I = 4 then
  begin
    Names := ' "hosted.json" "ledger.json" "host.key"';
    Opt := '';
  end
  else
  begin
    Names := '';
    Opt := ' /E';
  end;
  case Mode of
    0: Params := '"' + Src + '" "' + Dst + '"' + Names + Opt + ' /COPY:DAT /R:1 /W:1 /NJH /NJS /NP /NFL /NDL';
    1: Params := '"' + Dst + '" "' + Src + '"' + Names + Opt + ' /XC /XN /XO /XX /R:1 /W:1 /NJH /NJS /NP /NFL /NDL';
  else
    Params := '"' + Dst + '" "' + Src + '"' + Names + Opt + ' /L /XX /R:1 /W:1 /NJH /NJS /NP /NFL /NDL';
  end;
  if Exec(ExpandConstant('{sys}\robocopy.exe'), Params, '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    Result := ResultCode
  else
    Result := -1;
end;

{ '' when the copy is made (or there is nothing to copy); otherwise the reason, which stops the install
  before anything is written. }
function BackupUserData(): String;
var
  I, Rc: Integer;
begin
  Result := '';
  BackupMade := False;
  BackupRoot := ExpandConstant('{userdocs}') + '\Cairn-backup\' + GetDateTimeString('yyyymmdd_hhnnss', #0, #0);
  for I := 0 to BackupSetCount - 1 do
  begin
    BackupHas[I] := BackupSetExists(I);
    if not BackupHas[I] then Continue;
    if not ForceDirectories(BackupRoot) then
    begin
      Result := 'The safety copy of your saves could not be made (folder ' + BackupRoot + ' cannot be created). Nothing was installed.';
      Exit;
    end;
    Rc := BackupRun(0, I);
    Log('backup: ' + BackupSetName(I) + ' -> robocopy exit ' + IntToStr(Rc));
    if (Rc < 0) or (Rc >= 8) then
    begin
      Result := 'The safety copy of your saves failed (' + BackupSetName(I) + ', code ' + IntToStr(Rc) + ') -- a full disk, or a file in use. ' +
                'Nothing was installed. Your saves are untouched.' + #13#10#13#10 + 'Partial copy: ' + BackupRoot;
      Exit;
    end;
    BackupMade := True;
  end;
  if BackupMade then Log('backup: copy made in ' + BackupRoot)
  else Log('backup: nothing to copy (no saves or mod data found)');
end;

{ After the install: put back anything missing, compare, delete the copy only when everything matches. }
procedure BackupFinish();
var
  I, Rc: Integer;
  AllSame: Boolean;
begin
  if not BackupMade then Exit;
  AllSame := True;
  for I := 0 to BackupSetCount - 1 do
  begin
    if not BackupHas[I] then Continue;
    Rc := BackupRun(1, I);
    if Rc = 1 then Log('backup: ' + BackupSetName(I) + ' had missing files; they were put back')
    else if (Rc < 0) or (Rc >= 8) then AllSame := False;
    Rc := BackupRun(2, I);
    { robocopy's exit code is a bit mask: 2 is only "an extra file in the original place" (a new save), not a loss. }
    if (Rc <> 0) and (Rc <> 2) then
    begin
      AllSame := False;
      Log('backup: ' + BackupSetName(I) + ' differs from the copy (listing exit ' + IntToStr(Rc) + ')');
    end;
  end;
  if AllSame then
  begin
    if DelTree(BackupRoot, True, True, True) then
    begin
      RemoveDir(ExpandConstant('{userdocs}') + '\Cairn-backup');   // only if now empty
      Log('backup: every original is present and identical; the copy was deleted');
    end
    else
      Log('backup: all identical, but the copy could not be deleted: ' + BackupRoot);
  end
  else
  begin
    Log('backup: kept in ' + BackupRoot);
    if not WizardSilent then
      MsgBox('Setup kept a safety copy of your saves because something in them looked different after the install:' + #13#10#13#10 +
             BackupRoot + #13#10#13#10 + 'Nothing was deleted. Check your saves, then delete that folder when you are happy.',
             mbInformation, MB_OK);
  end;
end;

{ Cairn: true when Setup was started by the launcher's update (/RELAUNCH=1). }
function RelaunchAfterInstall(): Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = '1';
end;

{ Cairn: a silent update (the launcher's own Update button) over an existing install never adds the desktop shortcut again: they have it, or removed it on purpose }
{ Cairn: whether this PC had a desktop shortcut, under this name or an earlier one, before Setup touched anything. [InstallDelete] removes
  the earlier ones before the icons are made, so a silent update has to remember it here and give the player the new one in their place. }
var
  HadDesktopIcon: Boolean;

function InitializeSetup(): Boolean;
begin
  HadDesktopIcon :=
    FileExists(ExpandConstant('{autodesktop}\{#ShortcutName}.lnk')) or
    FileExists(ExpandConstant('{autodesktop}\{#OldShortcutName}.lnk')) or
    FileExists(ExpandConstant('{autodesktop}\{#Old2ShortcutName}.lnk')) or
    FileExists(ExpandConstant('{autodesktop}\{#Old3ShortcutName}.lnk'));
  Result := True;
end;

function ShouldMakeDesktopIcon(): Boolean;
begin
  Result := HadDesktopIcon or not (WizardSilent and RegKeyExists(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{88C5B9F1-0E71-4D60-9418-5575D5684F95}_is1'));
end;

{ ------------------------------------------- Cairn: the mod's own data survives an uninstall

  The mod's data (henry, chests, start-save, hosted.json, ledger.json, host.key) lives in the install folder, which an uninstall
  removes whole. Before that, the uninstaller copies it to Documents\Cairn-kept (a copy, merged into what is already there); a later
  install puts back whatever is missing from the new folder and overwrites nothing. The game's own saves (Saved Games\kingdomcome2)
  are outside the install folder and are never touched. The copy is never deleted by Setup. }
function KeepModData(): Boolean;
var
  I, Rc: Integer;
begin
  Result := True;
  BackupRoot := ExpandConstant('{userdocs}') + '\Cairn-kept';
  for I := 1 to BackupSetCount - 1 do
  begin
    if not BackupSetExists(I) then Continue;
    if not ForceDirectories(BackupRoot) then begin Result := False; Exit; end;
    Rc := BackupRun(0, I);
    Log('uninstall: kept ' + BackupSetName(I) + ' in ' + BackupRoot + ' -> robocopy exit ' + IntToStr(Rc));
    if (Rc < 0) or (Rc >= 8) then begin Result := False; Exit; end;
  end;
end;

{ Cairn: the folder is Cairn-kept now; what an earlier Cairn uninstall kept in CAIRN-kept is put back too. }
procedure RestoreKeptDataFrom(const FolderName: String);
var
  I, Rc: Integer;
begin
  BackupRoot := ExpandConstant('{userdocs}') + '\' + FolderName;
  if not DirExists(BackupRoot) then Exit;
  for I := 1 to BackupSetCount - 1 do
  begin
    if not DirExists(BackupRoot + '\' + BackupSetName(I)) then Continue;
    Rc := BackupRun(1, I);
    Log('install: kept data ' + BackupSetName(I) + ' (' + FolderName + ') put back where missing -> robocopy exit ' + IntToStr(Rc));
  end;
end;

procedure RestoreKeptData();
begin
  RestoreKeptDataFrom('Cairn-kept');
  RestoreKeptDataFrom('CAIRN-kept');
end;

{ -------------------------------------------------------------- WebView2 }

function WebView2Installed(): Boolean;
var
  V: String;
begin
  Result := False;
  { Per-machine install writes the 32-bit view; per-user writes HKCU. Both are
    the documented locations, and the 32-bit one is where this dev machine's
    runtime (pv 150.0.4078.105) actually is. }
  if RegQueryStringValue(HKLM32, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', V) then
    if (V <> '') and (V <> '0.0.0.0') then Result := True;
  if not Result then
    if RegQueryStringValue(HKLM64, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', V) then
      if (V <> '') and (V <> '0.0.0.0') then Result := True;
  if not Result then
    if RegQueryStringValue(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', V) then
      if (V <> '') and (V <> '0.0.0.0') then Result := True;
end;

function OnDownloadProgress(const Url, FileName: String; const Progress, ProgressMax: Int64): Boolean;
begin
  if not WizardSilent then
  begin
    if ProgressMax > 0 then
      WizardForm.PreparingLabel.Caption :=
        'Downloading the Microsoft WebView2 runtime (' +
        IntToStr(Progress div 1024) + ' of ' + IntToStr(ProgressMax div 1024) + ' KB)...'
    else
      WizardForm.PreparingLabel.Caption :=
        'Downloading the Microsoft WebView2 runtime (' + IntToStr(Progress div 1024) + ' KB)...';
  end;
  Result := True;
end;

{ Returns '' on success, or a message explaining why setup cannot continue. }
function EnsureWebView2(): String;
var
  Bootstrapper: String;
  ResultCode: Integer;
begin
  Result := '';
  if WebView2Installed() then Exit;

  if not WizardSilent then
    WizardForm.PreparingLabel.Caption := 'Downloading the Microsoft WebView2 runtime...';

  Bootstrapper := ExpandConstant('{tmp}\MicrosoftEdgeWebview2Setup.exe');
  try
    DownloadTemporaryFile('{#WebView2BootstrapUrl}', 'MicrosoftEdgeWebview2Setup.exe', '', @OnDownloadProgress);
  except
    Result :=
      'The launcher needs the Microsoft WebView2 runtime, and it could not be downloaded:' + #13#10 +
      GetExceptionMessage + #13#10#13#10 +
      'Check your internet connection and try again, or install "Microsoft Edge WebView2 Runtime"' + #13#10 +
      'from microsoft.com and re-run this installer.';
    Exit;
  end;

  if not WizardSilent then
    WizardForm.PreparingLabel.Caption := 'Installing the Microsoft WebView2 runtime...';

  if not Exec(Bootstrapper, '/silent /install', '', SW_SHOW, ewWaitUntilTerminated, ResultCode) then
  begin
    Result := 'The WebView2 runtime installer could not be started (error ' + IntToStr(ResultCode) + ').';
    Exit;
  end;

  if not WebView2Installed() then
    Result :=
      'The WebView2 runtime installer finished (exit code ' + IntToStr(ResultCode) + ') but the' + #13#10 +
      'runtime is still not registered. The launcher cannot draw its window without it.' + #13#10#13#10 +
      'Install "Microsoft Edge WebView2 Runtime" from microsoft.com, then run this installer again.';
end;

{ ------------------------------------------------------------- mod folder }

{ DelTree is the normal path, but it was observed failing once on this folder
  while a handle was still open on it -- the same thing an Explorer window, a
  running game or an antivirus scan will do on a user's machine. So: try,
  give the handle a moment to close, try again, then fall back to rd, and
  judge the outcome by whether the folder is actually gone rather than by the
  return value. }
function RemoveModFolder(const Target: String): Boolean;
var
  ResultCode: Integer;
begin
  if DelTree(Target, True, True, True) and (not DirExists(Target)) then
  begin
    Result := True;
    Exit;
  end;

  Sleep(750);
  DelTree(Target, True, True, True);
  if not DirExists(Target) then
  begin
    Result := True;
    Exit;
  end;

  Exec(ExpandConstant('{cmd}'), '/c rd /s /q "' + Target + '"', '', SW_HIDE,
       ewWaitUntilTerminated, ResultCode);
  Result := not DirExists(Target);
end;

{ ------------------------------------------------------------ wizard flow }

procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = wpFinished then
    WizardForm.FinishedLabel.Caption :=
      'Cairn Client is installed.' + #13#10#13#10 +
      'Open the launcher (leave the box below ticked), pick a game and it downloads' + #13#10 +
      'that game''s mod and does whatever else the game needs, step by step, in the' + #13#10 +
      'background. You can keep using your PC meanwhile, and close the launcher at' + #13#10 +
      'any time: it carries on where it stopped the next time you open it.';
end;
{ Kills this project's own processes (never the game). Shared by the silent
  install gate below and by silent uninstall -- unattended means unattended,
  and the launcher only writes settings.json when the user presses Save, so
  there is no in-flight state to lose. }
procedure KillOursQuietly();
var
  I, ResultCode: Integer;
  Names: array[0..3] of String;
begin
  Names[0] := 'Cairn.Launcher.exe';
  Names[1] := 'CairnClient.exe';
  Names[2] := 'CairnServer.exe';
  { the launcher starts this one itself, it runs out of
    the MasterServer subfolder of the install dir, and it was in none of the three process lists. Inno's
    RestartManager does close it in practice -- observed on 2026-08-28 -- but
    RM is a courtesy, not a guarantee: it cannot reach a process in another
    session, and a process is free to ignore the shutdown request. }
  Names[3] := 'CairnMasterServer.exe';
  for I := 0 to 3 do
    Exec(ExpandConstant('{cmd}'), '/c taskkill /f /im "' + Names[I] + '"',
         '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

{ NPC sync follow-up -- the install-time process gate.

  A real 0.11.8 install half-applied: Setup ran while agent/relay processes
  were alive, CloseApplications did not catch them (they were running from a
  dev tree rather than the install directory, but the DLLs still failed to
  be overwritten, without an error), and the result was a
  launcher on the new build with an agent on the old one -- the newly-shipped
  feature silently inert. See So: refuse to install
  while ANY of this project's processes, or the game, is running, no matter
  where they run from. Interactive gets Retry/Cancel; silent kills our own
  processes (mirroring InitializeUninstall's documented behaviour) but never
  the game -- an unattended install has no business closing someone's game. }
function FirstInstallBlocker(): String;
var
  I, ResultCode: Integer;
  Names: array[0..4] of String;
begin
  Result := '';
  Names[0] := 'Cairn.Launcher.exe';
  Names[1] := 'CairnClient.exe';
  Names[2] := 'CairnServer.exe';
  Names[3] := 'CairnMasterServer.exe';   { see KillOursQuietly }
  Names[4] := 'KingdomCome.exe';

  for I := 0 to 4 do
    if Exec(ExpandConstant('{cmd}'),
            '/c tasklist /fi "imagename eq ' + Names[I] + '" /nh | find /i "' + Names[I] + '"',
            '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
      if ResultCode = 0 then
      begin
        Result := Names[I];
        Exit;
      end;
end;

{ Returns '' when clear to install, or an abort message. }
function EnsureNothingRunning(): String;
var
  Blocker: String;
  OursClosed: Boolean;
begin
  Result := '';
  OursClosed := False;

  Blocker := FirstInstallBlocker();
  while Blocker <> '' do
  begin
    { Cairn: our own programs (launcher, agent, relay, master) are closed by Setup itself, once, without asking -- they hold
      no unsaved state (the launcher writes its settings only when Save is pressed). The game is never closed for the player:
      it is asked for below. }
    if (not WizardSilent) and (not OursClosed) and (Blocker <> 'KingdomCome.exe') then
    begin
      Log('install: closing the mod''s own running programs (' + Blocker + ' was running)');
      KillOursQuietly();
      Sleep(1500);
      OursClosed := True;
      Blocker := FirstInstallBlocker();
    end
    else if WizardSilent then
    begin
      if Blocker = 'KingdomCome.exe' then
      begin
        Result := 'The game (KingdomCome.exe) is running. Setup cannot safely replace files while it is. Close the game and run Setup again.';
        Exit;
      end;
      KillOursQuietly();
      Sleep(1500);
      Blocker := FirstInstallBlocker();
      if Blocker <> '' then
      begin
        Result := Blocker + ' is still running and could not be closed. Close it and run Setup again.';
        Exit;
      end;
      Break;
    end
    { No hard line breaks inside sentences: MsgBox uses a proportional font
      and wraps on its own; manual mid-sentence breaks made the real dialog
      ragged (seen on screen). Breaks only between paragraphs. }
    else
    begin
      if MsgBox(Blocker + ' is still running.' + #13#10#13#10 +
                'Installing over running programs is how an update half-applies: some files update, the ones in use silently do not, and the result looks installed but is a mix of two versions.' + #13#10#13#10 +
                'Close it (the game if it is open), then click Retry.',
                mbConfirmation, MB_RETRYCANCEL) <> IDRETRY then
      begin
        Result := 'Setup was cancelled because ' + Blocker + ' was still running.';
        Exit;
      end;
      Blocker := FirstInstallBlocker();
    end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';

  { The process gate, before anything is written -- see its comment above. }
  Result := EnsureNothingRunning();
  if Result <> '' then Exit;

  { Cairn: the safety copy of the saves and the mod's own data, before anything is written. }
  Result := BackupUserData();
  if Result <> '' then Exit;

  Result := EnsureWebView2();
end;

{ ================================================ verify, sweep and repair }

{ NPC sync follow-up, rebuilt in the install proves itself before
  declaring success, and repairs what it can before judging.

  Build-Installer.ps1 writes install-manifest.txt into the payload after
  publish (APP|<rel>|<size>|<sha256> for the install dir, MOD|... for the two files in
  the game folder), so it ships inside the install directory and describes
  exactly what this Setup carried.

  Three things changed in, each closing a way the old check could
  report PASS over a broken install:

    * sha256, not just size. A stale file that happened to match on length
      passed. "Every stale-vs-built pair observed differed in size" was luck.
    * the mod half is verified too. It lands outside the install dir and was checked by
      nothing at all -- a run that left the game with an EMPTY mod folder
      still wrote PASS (observed 2026-08-28, Setup 0.18.8).
    * the install dir is a CLOSED set. The manifest used to be a whitelist, so any file
      that no release ships -- a hand `dotnet publish` into the install
      directory, an assembly from an older layout -- sat there forever,
      invisible to the check and perfectly able to break assembly loading.
      That is the shape of the relay that could not cold-start in.
      Managed extensions (.dll .exe .pdb .deps.json .runtimeconfig.json) that
      are not in the manifest are now deleted and named in the report.

  Deliberately NOT swept: everything else. settings.json, favorites.json,
  custom_servers.json, the logs, and anything a user dropped in there are
  none of Setup's business, and an installer that eats user files to tidy up
  is a worse bug than the one being fixed. }

function PipeField(const S: String; Index: Integer): String;
var
  I, Start, Count: Integer;
begin
  Result := '';
  Start := 1;
  Count := 0;
  for I := 1 to Length(S) + 1 do
    if (I > Length(S)) or (S[I] = '|') then
    begin
      if Count = Index then
      begin
        Result := Copy(S, Start, I - Start);
        Exit;
      end;
      Count := Count + 1;
      Start := I + 1;
    end;
end;

{ '' on success, or why the manifest could not be read. }
function LoadManifest(): String;
var
  Lines: TArrayOfString;
  I, NA, NM: Integer;
  Line, Kind, Rel: String;
begin
  Result := '';
  SetArrayLength(ManApp, 0);
  SetArrayLength(ManMod, 0);
  AppIndex := '|';
  NA := 0;
  NM := 0;

  if not LoadStringsFromFile(ExpandConstant('{app}\install-manifest.txt'), Lines) then
  begin
    Result := 'install-manifest.txt is missing from the install folder';
    Exit;
  end;

  for I := 0 to GetArrayLength(Lines) - 1 do
  begin
    Line := Trim(Lines[I]);
    if Line = '' then Continue;
    if Line[1] = '#' then Continue;
    Kind := PipeField(Line, 0);
    Rel := PipeField(Line, 1);
    if Rel = '' then Continue;
    if CompareText(Kind, 'APP') = 0 then
    begin
      NA := NA + 1;
      SetArrayLength(ManApp, NA);
      ManApp[NA - 1] := Rel + '|' + PipeField(Line, 2) + '|' + PipeField(Line, 3);
      AppIndex := AppIndex + Lowercase(Rel) + '|';
    end
    else if CompareText(Kind, 'MOD') = 0 then
    begin
      NM := NM + 1;
      SetArrayLength(ManMod, NM);
      ManMod[NM - 1] := Rel + '|' + PipeField(Line, 2) + '|' + PipeField(Line, 3);
    end;
  end;

  if NA = 0 then
    Result := 'install-manifest.txt lists no files -- it is not a v2 manifest';
end;

function EndsWithText(const S, Suffix: String): Boolean;
begin
  Result := (Length(S) > Length(Suffix)) and
            (CompareText(Copy(S, Length(S) - Length(Suffix) + 1, Length(Suffix)), Suffix) = 0);
end;

{ Files whose presence can change what the .NET runtime loads. Nothing else
  is ever a sweep candidate. unins* is Inno's own and is not in the manifest
  by definition. }
function IsSweepCandidate(const Name: String): Boolean;
begin
  Result := False;
  if CompareText(Copy(Name, 1, 5), 'unins') = 0 then Exit;
  Result := EndsWithText(Name, '.dll') or
            EndsWithText(Name, '.exe') or
            EndsWithText(Name, '.pdb') or
            EndsWithText(Name, '.deps.json') or
            EndsWithText(Name, '.runtimeconfig.json');
end;

procedure SweepDir(const Base, Rel: String; var Removed: TArrayOfString; var Count: Integer);
var
  FindRec: TFindRec;
  Dir, RelName, Full: String;
begin
  if Rel = '' then Dir := Base else Dir := Base + '\' + Rel;
  if not FindFirst(Dir + '\*', FindRec) then Exit;
  try
    repeat
      if (FindRec.Name = '.') or (FindRec.Name = '..') then Continue;
      if Rel = '' then RelName := FindRec.Name else RelName := Rel + '\' + FindRec.Name;
      Full := Base + '\' + RelName;

      if (FindRec.Attributes and FILE_ATTRIBUTE_DIRECTORY) <> 0 then
      begin
        { the downloaded mods live in packs\ and their programs (.exe, .dll) are not the launcher's: never swept. Without this every
          update deleted them, and each mod then asked to be downloaded again. }
        if (Rel = '') and ((CompareText(FindRec.Name, 'packs') = 0) or (CompareText(FindRec.Name, 'cache') = 0)) then Continue;
        SweepDir(Base, RelName, Removed, Count)
      end
      else if IsSweepCandidate(FindRec.Name) then
        if Pos('|' + Lowercase(RelName) + '|', AppIndex) = 0 then
        begin
          ClearReadOnly(Full);
          if DeleteFile(Full) then
          begin
            Count := Count + 1;
            SetArrayLength(Removed, Count);
            Removed[Count - 1] := RelName;
            Log('repair: removed stale ' + RelName);
          end
          else
            Log('repair: could NOT remove stale ' + RelName);
        end;
    until not FindNext(FindRec);
  finally
    FindClose(FindRec);
  end;
end;

{ True when the file matches; Detail describes the mismatch when it does not. }
function VerifyEntry(const Base, Entry: String; var Detail: String): Boolean;
var
  Rel, Full, WantHash, GotHash: String;
  WantSize, GotSize: Int64;
begin
  Result := False;
  Rel := PipeField(Entry, 0);
  WantSize := StrToInt64Def(PipeField(Entry, 1), -1);
  WantHash := Lowercase(PipeField(Entry, 2));
  Full := Base + '\' + Rel;

  if not FileExists(Full) then
  begin
    Detail := Rel + '  (missing)';
    Exit;
  end;

  GotSize := -1;
  if not FileSize64(Full, GotSize) then GotSize := -1;
  if GotSize <> WantSize then
  begin
    Detail := Rel + '  (' + IntToStr(GotSize) + ' bytes, expected ' + IntToStr(WantSize) + ')';
    Exit;
  end;

  { A file held open by something else throws here rather than returning a
    hash, and that is a failure worth naming, not one worth crashing on. }
  try
    GotHash := Lowercase(GetSHA256OfFile(Full));
  except
    Detail := Rel + '  (could not be read -- something is using it)';
    Exit;
  end;

  if GotHash <> WantHash then
  begin
    Detail := Rel + '  (wrong content -- sha256 ' + Copy(GotHash, 1, 12) +
              ', expected ' + Copy(WantHash, 1, 12) + ')';
    Exit;
  end;

  Result := True;
end;

{ Written before a single file is copied, so that a Setup which dies half way
  can never leave the PREVIOUS run's PASS sitting there as the record of what
  happened. Observed doing exactly that on 2026-08-28: an aborted 0.18.8
  upgrade left install-verify.txt reading PASS over an install it had just
  half-replaced and a game folder it had just emptied. }
procedure StampVerifyInProgress();
var
  Verdict: TArrayOfString;
begin
  ForceDirectories(ExpandConstant('{app}'));
  SetArrayLength(Verdict, 2);
  Verdict[0] := 'FAIL  install in progress -- Setup {#AppVersion} has not finished';
  Verdict[1] := '  If this line is still here, Setup stopped before it could verify anything.';
  SaveStringsToFile(ExpandConstant('{app}\install-verify.txt'), Verdict, False);
end;

procedure VerifyInstalledFiles();
var
  Failures, Removed, Verdict: TArrayOfString;
  I, FailCount, RemovedCount, Line: Integer;
  Detail, LoadError: String;
  Gone: Boolean;
begin
  VerifyFailed := False;
  FailCount := 0;
  RemovedCount := 0;
  SetArrayLength(Failures, 0);
  SetArrayLength(Removed, 0);

  LoadError := LoadManifest();
  if LoadError <> '' then
  begin
    { Every release since ships one. Its absence IS a broken install. }
    FailCount := 1;
    SetArrayLength(Failures, 1);
    Failures[0] := LoadError;
    Log('verify FAIL: ' + LoadError);
  end
  else
  begin
    { Repair first, judge second -- a stale assembly that the sweep removes is
      not a failure, it is a fixed install. }
    SweepDir(ExpandConstant('{app}'), '', Removed, RemovedCount);

    for I := 0 to GetArrayLength(ManApp) - 1 do
      if not VerifyEntry(ExpandConstant('{app}'), ManApp[I], Detail) then
      begin
        FailCount := FailCount + 1;
        SetArrayLength(Failures, FailCount);
        Failures[FailCount - 1] := Detail;
        Log('verify FAIL: ' + Detail);
      end;


  end;

  { The verdict file is the record every tool reads: tools\Verify-Install.ps1,
    tools\Test-InstallerUpgrade.ps1, and anyone triaging a tester's machine. }
#ifdef ReleaseCandidate
  SetArrayLength(Verdict, FailCount + RemovedCount + 4);
#else
  SetArrayLength(Verdict, FailCount + RemovedCount + 3);
#endif
  Line := 0;
  if FailCount = 0 then
    Verdict[0] := 'PASS  ' + IntToStr(GetArrayLength(ManApp)) +
                  ' component(s) verified by sha256 against the install manifest; mod held back for the launcher'
  else
    Verdict[0] := 'FAIL  ' + IntToStr(FailCount) + ' component(s) did not install correctly:';
  Line := 1;
  for I := 0 to FailCount - 1 do
  begin
    Verdict[Line] := '  ' + Failures[I];
    Line := Line + 1;
  end;
  Verdict[Line] := 'version {#AppVersion}   repaired ' + IntToStr(RemovedCount) +
                   ' stale file(s) that no release ships';
  Line := Line + 1;
  for I := 0 to RemovedCount - 1 do
  begin
    Verdict[Line] := '  removed ' + Removed[I];
    Line := Line + 1;
  end;
  // the launcher downloads each game's mod and its pack's setup places it; Setup never does
  Verdict[Line] := 'mod held back: the launcher downloads it, then places it once the game is set up';
#ifdef ReleaseCandidate
  // tools\Build-Installer.ps1 -ReleaseCandidate; read by tools\Verify-Install.ps1.
  Verdict[Line + 1] := 'not soak-tested: release candidate, not for public release';
  Log('verify: not soak-tested: release candidate, not for public release');
#endif
  SaveStringsToFile(ExpandConstant('{app}\install-verify.txt'), Verdict, False);

  if FailCount = 0 then
  begin
    Log('verify: PASS (' + IntToStr(GetArrayLength(ManApp)) +
        ' components, ' + IntToStr(RemovedCount) + ' stale removed)');
    Exit;
  end;

  { Loud, named, and non-zero. An installer that can end in a silent
    half-state is the bug class this work order exists to close, so a failure
    here is never allowed to look like success. }
  VerifyFailed := True;
  if not WizardSilent then
  begin
    Detail := '';
    for I := 0 to FailCount - 1 do
      if I < 8 then Detail := Detail + #13#10 + '    ' + Failures[I];
    if FailCount > 8 then
      Detail := Detail + #13#10 + '    ... and ' + IntToStr(FailCount - 8) + ' more';

    { "-1 bytes" = the file is not there right after Setup copied it. Nothing "using" a file
      does that; an antivirus or Windows Security removing it does (the field's
      CairnInjector.exe, -1 bytes, on fresh installs). Say which it is. }
    Gone := False;
    for I := 0 to FailCount - 1 do
      if Pos('(-1 bytes', Failures[I]) > 0 then Gone := True;
    if Gone then
      MsgBox('THIS INSTALL IS NOT COMPLETE.' + #13#10#13#10 +
             IntToStr(FailCount) + ' component(s) did not install correctly:' + Detail + #13#10#13#10 +
             'A file shown with -1 bytes was removed right after Setup copied it. That is almost always ' +
             'Windows Security or another antivirus. Open Windows Security > Virus & threat protection > ' +
             'Protection history (or your antivirus), restore the file and allow it, then run this installer ' +
             'again. Setup never changes your security settings.' + #13#10#13#10 +
             'The full list is in install-verify.txt in the install folder.',
             mbCriticalError, MB_OK)
    else
    MsgBox('THIS INSTALL IS NOT COMPLETE.' + #13#10#13#10 +
           IntToStr(FailCount) + ' component(s) did not install correctly:' + Detail + #13#10#13#10 +
           'Almost always this means something was still using those files. Close the launcher, ' +
           'the agent, the relay, the master server and the game, then run this installer again -- ' +
           'it repairs an install in this state.' + #13#10#13#10 +
           'The full list is in install-verify.txt in the install folder.',
           mbCriticalError, MB_OK);
  end;
end;

// the dice keys. Setup carries none of the game's files: the agent's
// --keys-pak verb reads the player's own Libs/Config/defaultProfile.xml and
// keybindSuperactions.xml out of the game's Data paks, adds our lines, and
// writes Mods\cairn\Data\cairn_keys.pak (dotnet\Cairn.Client\KeybindPak.cs).
// Not part of the verdict: a failure costs the keys only (the console commands
// still work), and the launcher runs it again before every game start.
procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    StampVerifyInProgress()
  else if CurStep = ssPostInstall then
  begin
    VerifyInstalledFiles();
    BackupFinish();
    RestoreKeptData();   // Cairn: what an earlier uninstall kept, where it is missing
  end;
end;

{ Runs after the wizard is finished either way. See the ExitProcess comment. }
procedure DeinitializeSetup();
begin
  if VerifyFailed then
    ExitProcess(101);
end;

{ ----------------------------------------------------------- uninstalling }

// True if any of ours is still running. Inno's own CloseApplications drives
// the Restart Manager from the Files section, which does not cover the
// UninstallDelete sweep of the install directory -- so uninstalling with the
// launcher open removed the registry entries and the uninstaller itself but
// left ~120 files of ~680 behind. Observed on a real run, not theorised.
//
// tasklist rather than a mutex, because a mutex would mean changing the
// launcher and the uninstaller has no business requiring that. `find`
// returns exit code 1 when it matches nothing, which is the whole test.
function AnyOfOursRunning(): Boolean;
var
  I, ResultCode: Integer;
  Names: array[0..3] of String;
begin
  Result := False;
  Names[0] := 'Cairn.Launcher.exe';
  Names[1] := 'CairnClient.exe';
  Names[2] := 'CairnServer.exe';
  Names[3] := 'CairnMasterServer.exe';   { see KillOursQuietly }

  for I := 0 to 3 do
    if Exec(ExpandConstant('{cmd}'),
            '/c tasklist /fi "imagename eq ' + Names[I] + '" /nh | find /i "' + Names[I] + '"',
            '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
      if ResultCode = 0 then
      begin
        Result := True;
        Exit;
      end;
end;

{ Runs before anything is removed, and returning False aborts cleanly with
  nothing touched -- which is the point: a half-finished uninstall is worse
  than one that did not start. }
function InitializeUninstall(): Boolean;
var
  OursClosed: Boolean;
begin
  Result := True;
  OursClosed := False;

  while AnyOfOursRunning() do
  begin
    { Cairn: the mod's own programs are closed by the uninstaller itself, once; only if one stays does it ask. }
    if (not UninstallSilent) and (not OursClosed) then
    begin
      KillOursQuietly();
      Sleep(1500);
      OursClosed := True;
    end
    else if UninstallSilent then
    begin
      { Nobody to ask. Unattended means unattended, and the launcher only
        writes settings.json when the user presses Save, so there is no
        in-flight state to lose. }
      KillOursQuietly();
      Sleep(1500);
      if AnyOfOursRunning() then
      begin
        Result := False;
        Exit;
      end;
      Break;
    end
    else if MsgBox('Cairn Client is still running.' + #13#10#13#10 +
              'Close the launcher (and the game, if it is open) first, then click Retry.' + #13#10 +
              'Uninstalling now would leave files behind that Windows will not let it delete.',
              mbConfirmation, MB_RETRYCANCEL) <> IDRETRY then
    begin
      Result := False;
      Exit;
    end;
  end;

  { Cairn: the mod's own data is copied to Documents\Cairn-kept before anything is removed; if that cannot be done, nothing is. }
  if not KeepModData() then
  begin
    if not UninstallSilent then
      MsgBox('Your characters, chests and start save could not be copied to Documents\Cairn-kept, so nothing was removed.' + #13#10#13#10 +
             'Free some disk space or close whatever uses those files, then try again.', mbError, MB_OK);
    Result := False;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Mods: String;
begin
  // Belt and braces for the case InitializeUninstall cannot catch: something
  // grabbed a file mid-uninstall. Better a plain sentence than silent debris
  // in the user's profile. Tested on the launcher exe rather than on the
  // install directory, which legitimately still holds unins000.exe here.
  if CurUninstallStep = usPostUninstall then
  begin
    if FileExists(ExpandConstant('{app}\Cairn.Launcher.exe')) and (not UninstallSilent) then
      MsgBox('Some files were in use and could not be removed. They are still in:' + #13#10#13#10 +
             ExpandConstant('{app}') + #13#10#13#10 +
             'Nothing there is needed any more -- deleting that folder by hand finishes the job.',
             mbInformation, MB_OK);
    Exit;
  end;

  if CurUninstallStep <> usUninstall then Exit;

  { The mod lives inside the game's own folder, so it is never removed
    silently -- and never without asking, unless this is an unattended run. }
  if not RegQueryStringValue(HKCU, 'Software\Cairn', 'ModsPath', Mods) then Exit;
  if (Mods = '') or (not DirExists(Mods)) then Exit;

  { Cairn: the launcher's Uninstall button runs this uninstaller with /REMOVEMOD=1: the mod goes too, without asking again. }
  if ExpandConstant('{param:REMOVEMOD|0}') = '1' then
  begin
    RemoveModFolder(Mods);
    Exit;
  end;

  if UninstallSilent then Exit;

  if MsgBox('Also remove the multiplayer mod from your game?' + #13#10#13#10 + Mods + #13#10#13#10 +
            'Choosing No leaves the mod installed; the game itself is not touched either way.',
            mbConfirmation, MB_YESNO) = IDYES then
    RemoveModFolder(Mods);
end;
