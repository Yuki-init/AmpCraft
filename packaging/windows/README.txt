AmpCraft for Windows (early test build)
======================================

A reimplementation of a classic skinnable music player. This is an early build: everything
works, but it has had little testing on real Windows machines yet.

Running
-------
Unzip anywhere and run ampcraft.exe. Nothing is installed. Windows may warn that the program
is unrecognized because it is not signed: choose "More info", then "Run anyway".

  ampcraft.exe [FILES or FOLDERS...] [--skin THEME] [--scale 1|2|3] [--audio null]

Settings are kept in %USERPROFILE%\.config\ampcraft, the library database and cache in
%LOCALAPPDATA%\ampcraft. Delete those folders to remove all traces.

Testing
-------
test-kit\ holds an automated check that writes a report with screenshots. Open that folder
in Claude Code and ask it to run the test report (it follows test-kit\CLAUDE.md), or run

  powershell -ExecutionPolicy Bypass -File test-kit\run-report.ps1

and send back test-kit\report.

Licenses
--------
AmpCraft is free software under your choice of the MIT License (LICENSE-MIT.txt) or the
Apache License 2.0 (LICENSE-APACHE.txt). Source: https://github.com/Yuki-init/AmpCraft

The FFmpeg DLLs in this folder are FFmpeg under the GNU LGPL version 3, unmodified and
dynamically linked: see FFMPEG-LICENSE.txt and FFMPEG-SOURCE.txt for the exact build and
where its source is.
