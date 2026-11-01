# S4-DESKTOP

Date: 1 November 2026. Host: Linux x86_64. The desktop window was checked in Chrome against the Vite page at `http://127.0.0.1:1420/`, outside the Tauri shell, so the engine calls correctly reported that the desktop window was required.

## Checks

- `cargo test -p stackyard-core -p stackyard-engine -p stackyard-cli -p stackyard-desktop` passed. The desktop tests cover Quit leaving projects running only when background mode is `on`, editor commands rejecting shell syntax, and loopback URLs.
- `npm --prefix apps/desktop run build` passed.
- Chrome walked Projects, Components, and Settings, including a 420px-wide layout. Empty states are visible. Settings stayed disabled without the engine. No page errors.

## Behavior in the shell

Closing the main window hides it. The tray menu can show the window again. Quit stops managed projects unless Settings has “Leave projects running when quitting” turned on. That choice is the `background_mode` setting.

Dependency actions are labeled as project package changes. The project screen names the manager separately from the selected runtime.

## Not evidenced here

A live Tauri window on Windows 10 or macOS 14.5, including the tray icon and a full import-start-log-stop pass through that window.
