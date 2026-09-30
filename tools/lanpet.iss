; Windows installer (Inno Setup 6): per-user, so no admin prompt; Start menu entry and uninstaller.
; Built by tools/build-windows.sh into dist\LanPet-setup.exe. The in-app updater runs it with
; /VERYSILENT, which also closes the running LanPet and starts the new one.

[Setup]
AppId=io.github.luannzin.lanpet
AppName=LanPet
AppVersion={#Version}
AppPublisher=luannzin
AppPublisherURL=https://github.com/luannzin/lanpet
DefaultDirName={autopf}\LanPet
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=force
OutputDir=..\dist
OutputBaseFilename=LanPet-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\lanpet.ico
WizardStyle=modern

[Files]
Source: "..\target\release\lanpet.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\icon.ico"; DestDir: "{app}"; DestName: "lanpet.ico"

[Icons]
Name: "{autoprograms}\LanPet"; Filename: "{app}\lanpet.exe"; IconFilename: "{app}\lanpet.ico"

[Run]
Filename: "{app}\lanpet.exe"; Description: "Start LanPet"; Flags: nowait postinstall
