# OpenAMP Windows test run

You are on the owner's Windows machine, testing an early Windows build of OpenAMP, a
reimplementation of a classic 2.x-era skinnable music player. The build was cross-compiled
on Linux and has only been tested under Wine, so this is the first run on real Windows. Your
job: run the checks, look at the screenshots, investigate anything odd, and write a report
the developers can act on.

## Layout

```
openamp-<version>-windows-x64\
  openamp.exe, openamp-cli.exe     the player and its headless CLI
  av*.dll, swresample-*.dll        FFmpeg (LGPL), loaded from this folder
  test-kit\                        you are here
    run-report.ps1                 automated checks and screenshots
    report\                        created by the script; everything you write goes here
```

## Rules

- Write only inside `test-kit\report\`. The script points the player's settings, data and
  cache at `report\sandbox` with `OPENAMP_*_DIR` variables; keep that for anything you run
  by hand too (`. .\run-report.ps1 -LoadOnly` sets them for you via `Start-Player`).
- Do not install anything, change system settings, or touch the owner's music or files.
- Keep sound quiet and short: the one audible check is a 3 second tone at volume 20. Use
  `--audio null` for everything else, unless the owner asks for more.
- Leave no player running when you finish (`Stop-Player`, or close it from the taskbar).
- Do not put the owner's user name, machine name or personal paths in the report; write
  `%USERPROFILE%` or `<user>` instead.

## Steps

1. Run the automated checks from this folder:

   ```
   powershell -ExecutionPolicy Bypass -File .\run-report.ps1
   ```

   If Windows blocked the files (they came from the internet), run
   `Get-ChildItem -Recurse .. | Unblock-File` first. If SmartScreen stops `openamp.exe`
   (it is not signed), tell the owner; do not try to get around it yourself.

2. Read `report\results.json`, `report\system.txt`, `report\cli.txt`, the
   `report\windows-*.txt` window lists and the `report\player-*.log` files (the player's
   own log at info level).

3. Look at every screenshot in `report\` (the `*-player.png` crops are the useful ones).
   What correct looks like:
   - Three windows stacked and touching, top to bottom: the main window (275x116 at 1x,
     550x232 at 2x, 825x348 at 3x; the player picks 2x by itself on screens about 1300 px
     tall or more), the equalizer (same width, 116 high at 1x) and the
     playlist (same width, at least 116 high at 1x). No window frames, title bars or
     shadows from Windows; the skin draws its own title bars.
   - The default skin is a generated dark theme (Dracula colors): purple, pink and grey
     on near-black, with a pixel font. Nothing should be black, garbled, offset,
     stretched or blurry; at 2x every pixel is a sharp 2x2 block.
   - `02-dragged`: all three windows moved together by the same amount as the drag.
   - `03-menu`: a small menu next to the main window, drawn in the skin's colors.
   - `04-scale2-with-track`: double size; the main window shows a track title (tone)
     scrolling or static, and the time display.

4. Investigate failures and anything that looks wrong. The helpers are there for this:

   ```
   . .\run-report.ps1 -LoadOnly
   Start-Player @('--audio', 'null')      # or add '--scale', '2', a file, '--library'
   Get-PlayerWindows | Format-Table title, x, y, w, h, dpi
   Invoke-Click 100 50; Invoke-Click 100 50 -Right; Invoke-Drag 10 5 300 200
   Send-Keys @(0x58)                       # virtual-key codes: 0x58 X (play), 0x56 V (stop)
   Save-Screenshot '10-what-i-tried'
   Stop-Player
   ```

   Worth trying if the basics work: the main window's buttons (play, pause, stop, next,
   eject, shuffle, repeat), the volume slider, opening the equalizer and playlist from the
   EQ/PL buttons, dragging a panel away from the stack and back (it should snap within
   about 10 px), resizing the playlist from its bottom-right corner, window shade
   (double-click a title bar), Alt+S (next theme), Alt+L (media library), Ctrl+P
   (preferences), minimizing from the main window and restoring from the taskbar, and,
   if the machine has several monitors or a display scale above 100%, how the windows
   look and move there.

   Changed since the last report, please check by hand:
   - Japanese (and other CJK) titles: load the `テスト ü.wav` the script makes and look at
     the playlist, the main window title and the library. They should read テスト ü, not
     boxes.
   - Dialogs should open over the player, not at the screen's top-left corner, and the
     player should not react to clicks while one is open. Try Eject, playlist ADD (FILE and
     DIR), EQ PRESETS save/load to file, Preferences "Load skin...", and File info.
   - Folder pickers (playlist ADD > DIR, library Add folder) should be the modern Explorer
     style, with captions "Add folder to playlist" and "Add folder to library".
   - After a run at 2x, `--scale 3` should keep the three windows docked and move the group
     up so the playlist's bottom edge is on screen.
   - EQ PRESETS: hover Save so its submenu opens, press Escape (submenu closes), Escape
     again (menu closes).

   - `--scale` on the command line is now for that run only: after a `--scale 3` run, a
     plain start goes back to the saved size (Automatic unless one was picked in
     Preferences or with the double-size button), with the windows still docked.

   Known gap: media keys are not wired on Windows.

5. Write `report\REPORT.md`:
   - Environment: Windows version, display scale(s), monitors, sound device (from
     `system.txt`).
   - Results: a table of every check from `results.json` plus anything you tested by hand,
     with pass/fail and one line of detail.
   - Screenshots: one or two sentences per screenshot on what it shows and whether it
     matches the description above. Name the files.
   - Problems: for each, what you did, what you expected, what happened, how often, and
     anything relevant from the logs. Most severe first (crash, then wrong behavior, then
     cosmetic).
   - Anything that surprised you, even if it is not clearly a bug.

6. Zip the report folder for the owner, then tell them where it is:

   ```
   Compress-Archive -Force -Path .\report\* -DestinationPath .\openamp-windows-report.zip
   ```

   The `sandbox` folder inside may be left out if it is large; it holds only generated
   files and the player's settings.
