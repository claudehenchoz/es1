## Downloads

| | |
|---|---|
| **Windows** (64-bit) | `ES1-@VERSION@-windows-x64-setup.exe`: installer, installs into `C:\Program Files\Common Files\VST3`<br>`ES1-@VERSION@-windows-x64.zip`: the plugin, to copy by hand |
| **macOS** 11+ (Apple silicon and Intel) | `ES1-@VERSION@-macos-universal.pkg`: installer, installs into `/Library/Audio/Plug-Ins/VST3`<br>`ES1-@VERSION@-macos-universal.zip`: the plugin, to copy by hand |
| **Linux** (x86-64, glibc 2.35+) | `ES1-@VERSION@-linux-x64.tar.gz`: the plugin and `install.sh` (installs into `~/.vst3`, or `/usr/lib/vst3` with `--system`) |

Each archive has an `INSTALL.txt` with the details.

The installers are not code-signed. On Windows, SmartScreen may warn about an unknown publisher:
choose **More info** and **Run anyway**. On macOS, if the package is blocked, open **System
Settings → Privacy & Security** and choose **Open Anyway**. If you install from the zip on macOS,
clear the quarantine flag as `INSTALL.txt` describes, or hosts will not load the plugin.

ES 1 is free software under the GNU GPL, version 3 or later. The complete source code for this
release is attached below as "Source code".
