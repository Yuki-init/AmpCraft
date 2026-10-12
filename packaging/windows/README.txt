OpenAMP for Windows (early test build)
======================================

A reimplementation of a classic skinnable music player. This is an early build: the player
runs, but some parts (file dialogs, message boxes, media keys) are not done on Windows yet.

Running
-------
Unzip anywhere and run openamp.exe. Nothing is installed. Windows may warn that the program
is unrecognized because it is not signed: choose "More info", then "Run anyway".

  openamp.exe [FILES or FOLDERS...] [--skin THEME] [--scale 1|2|3] [--audio null]
  openamp-cli.exe --help        headless player: probe, decode, play, library, themes

Settings are kept in %USERPROFILE%\.config\openamp, the library database and cache in
%LOCALAPPDATA%\openamp. Delete those folders to remove all traces.

Testing
-------
test-kit\ holds an automated check that writes a report with screenshots. Open that folder
in Claude Code and ask it to run the test report (it follows test-kit\CLAUDE.md), or run

  powershell -ExecutionPolicy Bypass -File test-kit\run-report.ps1

and send back test-kit\report.

Licenses
--------
OpenAMP is free software under the GNU General Public License, version 3 or later
(LICENSE.txt). Source: https://github.com/Yuki-init/OpenAMP

The FFmpeg DLLs in this folder are FFmpeg under the GNU LGPL version 3, unmodified and
dynamically linked: see FFMPEG-LICENSE.txt and FFMPEG-SOURCE.txt for the exact build and
where its source is.
