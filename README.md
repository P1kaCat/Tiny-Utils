# Tiny Utils

A utility mod for Tiny Glade. Currently includes multiplayer support, with more features planned.

## Current features

- **Multiplayer** — play Tiny Glade with others over the network

## Installation

1. Go to the [latest release](https://github.com/P1kaCat/Tiny-Utils/releases/latest)
2. Download `Install.zip`
3. Extract the ZIP at the root of your Tiny Glade game folder (where `tiny-glade.exe` is located)
4. Double-click `tinyutils.bat`
5. Click **Install Mod** — the installer will download and place `dxgi.dll` automatically

Do NOT download `dxgi.dll` manually from the releases page. The installer handles everything — downloading it yourself won't work, the `.bat` needs to place it in the right location for you.

## Uninstallation

1. Double-click `tinyutils.bat` in your game folder
2. Click **Uninstall Mod** — the uninstaller will remove `dxgi.dll` automatically

## Files

| File | Purpose |
|------|---------|
| `tinyutils.bat` | Launcher — double-click to open the Install/Uninstall menu |
| `tinyutils_setup.ps1` | PowerShell script (must be next to the `.bat`) |

Both files must be in the same folder. Keep them together after extracting.

## Notes

- If Tiny Glade is running during uninstall, the tool will close it automatically
- The mod requires `dxgi.dll` to be in the same folder as `tiny-glade.exe`
- Run `tinyutils.bat` again anytime to install or uninstall

## Troubleshooting

- **Could not delete dxgi.dll** — Close Tiny Glade via Task Manager, then run the uninstaller again
- **Download failed** — Check your internet connection and try again
- **Window doesn't appear** — Make sure both `tinyutils.bat` and `tinyutils_setup.ps1` are in the same folder
