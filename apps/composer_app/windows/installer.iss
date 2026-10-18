; Inno Setup script for the Windows installer (built by .github/workflows/release.yml):
;   iscc /DAppVersion=0.1.0 windows\installer.iss   (after flutter build windows --release)
; Installs per user by default (no admin), with a Start menu shortcut that carries the
; AppUserModelID the app's notifications use.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{5F0A1C2E-7C39-4E57-9A37-2D4C3F6E8B11}
AppName=composer
AppVersion={#AppVersion}
AppPublisher=composer
DefaultDirName={autopf}\composer
DefaultGroupName=composer
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\build\installer
OutputBaseFilename=composer-app-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
UninstallDisplayIcon={app}\composer.exe
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\build\windows\x64\runner\Release\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\composer"; Filename: "{app}\composer.exe"; AppUserModelID: "dev.composer.app"
Name: "{autodesktop}\composer"; Filename: "{app}\composer.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\composer.exe"; Description: "{cm:LaunchProgram,composer}"; Flags: nowait postinstall skipifsilent
