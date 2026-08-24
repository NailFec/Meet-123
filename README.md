# Meet-123

On Linux, Google Meet in Chrome can share a tab with tab audio, but sharing a whole screen or window with system audio does not work. **Meet-123** is a local relay for that gap.

It captures a monitor or window into a dedicated Chromium tab, injects the audio your computer is already playing into that tab, and you present that tab in Meet with "Also share tab audio" enabled. Capture uses your session's xdg-desktop-portal (niri, KDE Plasma, GNOME, and other portals each supply their own picker). Audio uses PipeWire or PulseAudio. No root, and the helper does not write compositor config such as niri `config.kdl` or KDE `kwinrc`.

Meet still re-encodes the stream. You can capture at 1440p and 60–120 fps; remote viewers are usually closer to 1080p and 15–30 fps. Do not capture the same screen that shows the relay tab, or you get a hall of mirrors. Dual monitors: full-screen the relay on screen A, capture screen B.

## Requirements

- Linux (Wayland preferred; X11 can still capture via Chrome)
- A Chromium-based browser: Chrome, Chromium, Microsoft Edge, Vivaldi, or Brave
- PipeWire (typical) or PulseAudio, with `pactl` and `parec` (often `libpulse` / `pulseaudio-utils`)
- A working screen-share portal: `xdg-desktop-portal-kde` on KDE, `xdg-desktop-portal-gnome` on niri and GNOME
- To build: Node.js (npm) and a Rust toolchain (`cargo`, `rustc`)

## Usage

### First build

From the repository root:

```bash
make run
```

This installs web dependencies, builds the static UI, compiles the helper, then starts it. The helper creates a silent sink named `Meet123Silent`, may show a tray icon (optional), and opens a Chromium-based browser on `http://127.0.0.1:17373`.

### Later runs

You do not need `make run` every time. After a successful build:

```bash
./helper/target/release/meet123
```

Rebuild with `make run` (or `make web` / `make helper`) only after you change the code. Leave the helper running while you present; stopping it stops system audio in the tab.

### Present in Google Meet

1. Put the relay tab on a screen you will not share. Use Present layout or press `F` to full-screen it.
2. Choose a quality preset, then Capture screen / window. In the desktop picker, select the other monitor or a window.
3. Start injecting system audio. The default source is whatever you currently hear. You can instead tick specific apps, or enable “exclude the browser” if isolating the relay stream fails.
4. In Meet: Present now → A tab → enable tab audio.
5. In Meet, turn on Optimize for motion/video, not text sharpness.

Switching away from the relay tab after Meet is presenting it is fine. On niri: niri only sends about 1 Hz frame callbacks to invisible windows, so Meet looks extremely stuttery. KDE is usually smoother on another virtual desktop because overview still wants thumbnails; do not minimize the window.

To fix the stuttery problem on niri, you can try these methods:
- Normal way, the problem might not exist.
- Put the browser page on the screen with even 1 px visible area.
- Open the browser with this app, which will include some flags to disable stuttery on the background.
- Use another browser, and open it with this app.

### Helper flags

```bash
./helper/target/release/meet123 --listen 127.0.0.1:17373
./helper/target/release/meet123 --browser vivaldi
./helper/target/release/meet123 --no-open
./helper/target/release/meet123 --no-tray
```

`--browser` also reads `MEET123_BROWSER`. If unset, the helper prefers Chrome/Chromium, then Vivaldi, then Brave, then Edge (not Chrome Unstable).

### Desktop notes

These are optional tips, not extra dependencies:

- **Any desktop:** use the system screen/window dialog that Chrome opens.
- **niri:** keep the relay window on a live output (the unused monitor). Off-screen workspaces refresh at about 1 Hz. You can pick Dynamic Cast Target and change the source with niri binds; `block-out-from "screencast"` can hide Meet or password windows. This project never edits `config.kdl`.
- **KDE Plasma:** another virtual desktop is usually OK; minimized windows often stutter. Use the portal-kde picker. This project never edits `kwinrc`.

## Contributing

### Development

Layout:

- [`web/`](web/) — SvelteKit static relay page
- [`helper/`](helper/) — user-session helper (PipeWire/Pulse capture, WebSocket PCM, tray, browser launch)

Frontend and helper together:

```bash
# terminal 1
cd helper && cargo run -- --no-open

# terminal 2
cd web && npm run dev
```

Vite proxies `/api` and `/ws` to `127.0.0.1:17373`. Check the UI with `cd web && npm run check`. The helper is a normal Cargo crate under `helper/`.

### AI usage

AI-written code is allowed. A human must review all of it before it is merged.

All prose must be written by a human. That includes user-facing strings in the repo, issue text, and pull request titles and bodies. Do not paste model output into those places.

Each pull request must name the AI tools and models used.

## License

This project is licensed under the **GNU General Public License v3.0** (GPLv3).

See the [LICENSE](LICENSE) file for the full text.

```
Copyright (C) 2026 NailFec

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.
```

