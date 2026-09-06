# Main Window Readability

Status: implemented for candidate 1.0.10, not released. Native acceptance pending.

## Operation

- Drag any main-window edge or corner to resize. The lower-right corner has a small grip.
- Open Settings > Appearance & window.
- Small: 360 x 560 logical pixels, 100% content zoom (existing default).
- Comfortable: 480 x 680 logical pixels, 125% content zoom.
- Large: 640 x 820 logical pixels, 150% content zoom.
- Zoom choices: 100%, 115%, 125%, 150%.
- Ctrl/Cmd + plus or minus steps the zoom; Ctrl/Cmd + 0 resets zoom only.
- Reset window restores both the small size and 100% zoom.
- Actual dimensions are constrained to the current display work area. At larger
  zoom levels, minimum dimensions grow to preserve the usable layout viewport.

Preferences are stored under `eggdone-window-preferences-v1` in localStorage.
Only logical width, height and content zoom are stored, not absolute position.
They are not synchronized to other devices. Corrupt values fall back to defaults.
Native API/persistence failures leave an error in the settings section and attempt
to restore the previous visual settings.

## Boundaries

The focus window retains its separate compact/expanded behavior. Closing or
defocusing the main panel still hides it rather than quitting. Tray positioning
continues using actual window dimensions. Display changes and focus restore
recheck work-area constraints. There is no new full-screen or pinned-window mode.

No sync schema, credentials, database, task or recurrence behavior is changed.

## Automated Evidence

- `pnpm test`: 155 tests passed, including 8 window normalization/fitting tests.
- `pnpm check`: Svelte/TypeScript validation.
- `pnpm i18n:check`: 547 aligned keys, hardcode check passed.
- `pnpm build`: production frontend build.
- `cargo check` and `cargo fmt -- --check`: passed; existing unused-field warning retained.
- `node scripts/test-window-ui.mjs`: 16 language/theme/width combinations plus
  presets, zoom, shortcuts, persistence, DPI conversion, display fitting, failed
  operation rollback, resize command and listener cleanup checks. Set
  `PLAYWRIGHT_PATH` to an available Playwright module; Microsoft Edge is used.
- Browser screenshots inspected for Chinese/light and English/dark settings.

The browser suite uses real Svelte components with a mocked native transport.
It does not establish real OS resizing, WebView zoom rendering or tray acceptance.

## Manual Acceptance

1. Start the desktop client, open it from the tray, and drag all edges/corners.
   Check no accidental hiding, clipped controls or unexpected exit.
2. Select each preset and zoom level. Check task menus, four quadrants, notes,
   attachment management, settings, and recurrence dialogs in both languages/themes.
3. Resize then immediately hide/show via tray. Confirm the size is retained.
   Quit from the tray and restart; confirm dimensions and zoom restore.
4. Check Ctrl/Cmd shortcuts while viewing and editing tasks/notes; verify 0 resets
   zoom while the reset button restores both size and zoom.
5. Test 100%, 150%, 200% system DPI and monitors with different DPI/work areas,
   including a monitor to the left of the primary and disconnect/reconnect.
6. Recheck tray positioning and auto-hide on Windows, Linux and macOS. Verify the
   separate focus window is unchanged. Use existing data without uninstall/reset.

Real native-window and cross-platform visual acceptance remains pending.
