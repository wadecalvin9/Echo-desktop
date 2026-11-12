# Echo Desktop

A pure-Rust desktop music player built with **Iced**, **Tokio**, **Symphonia**, and **CPAL**.

## Features

- **Frameless Dark Desktop UI:** Borderless custom titlebar with window drag and minimize/maximize/close controls, responsive three-pane shell, dynamic album-art color theming (`Dynamic`, `Dark`, `OLED Black`), and a cinema-style Expanded Lyrics Stage.
- **YouTube Music Streaming & Browse:** Personalized Home shelves & mood chips, Explore (New Releases, Moods & Genres), autocomplete search, Artist/Album/Playlist detail pages, adaptive radio queue, and optional cookie-based account sync.
- **Audio Engine & DSP:** Symphonia + CPAL playback with Echo loudness leveling (`-3 dB` reference), `+12 dB` soft-clipped boost, 3-band equalizer presets, spatial audio, silence skipping, transition fades, and a sleep timer.
- **Synced & Romanized Lyrics:** LRCLIB, KuGou, and Better Lyrics (TTML) fetching with click-to-seek synced lines, auto-scroll, and CJK/Hangul/Kana romanization.
- **Song Recognition (Shazam):** 16 kHz mono audio fingerprinting from WASAPI loopback or microphone input with speed-retry fallback and recognition history.
- **Offline Downloads, Cache & Local Media:** Disk LRU stream cache, `[youtube_id]` download folder import/migration, and local audio library scanning (`lofty` tags + folder tree view).
- **Integrations & Persistence:** SQLite library/history/stats (`rusqlite`), Last.fm Now Playing & scrobbling, Windows System Media Transport Controls (SMTC), and `.echobak` backup/restore.

## Workspace Layout

- `crates/echo-core`: Domain models, SQLite persistence (`rusqlite`), settings, stream LRU cache, local media & download scanners, `.echobak` backup/restore, and dynamic album-art color extraction.
- `crates/echo-fingerprint`: Shazam/SongRec 16 kHz mono audio fingerprinting, Shazam recognition client, and microphone/WASAPI loopback capture.
- `crates/echo-lyrics`: LRCLIB, KuGou, and Better Lyrics (TTML) clients, synced `.lrc` parser, and romanization.
- `crates/echo-lastfm`: Last.fm browser auth, Now Playing, scrobbling, and `track.getSimilar`.
- `crates/echo-youtube`: YouTube Music InnerTube API client, stream resolution, and chunked range buffer.
- `crates/echo-audio`: Symphonia + CPAL playback engine with loudness leveling, soft-clipped limiter, EQ, spatial widening, and silence gating.
- `crates/echo-app`: Iced desktop GUI (`Home`, `Explore`, `Library`, `Search`, `Recognize`, `Stats`, `Settings`, `Expanded Lyrics Stage`, and `Queue`).

## Keyboard Shortcuts

- `Ctrl+K` / `Ctrl+F`: Focus search bar
- `Space`: Play / Pause (when not typing in an input)
- `Left` / `Right`: Seek `-5s` / `+5s`
- `Up` / `Down`: Volume `+10%` / `-10%`
- `M`: Mute / Unmute
- `Esc`: Close search suggestions, exit Expanded Stage, or navigate back
- Media keys (`Play/Pause`, `Next`, `Previous`): Global/window playback control + Windows SMTC

## Building & Running (Windows)

```powershell
# Using LLVM-MinGW UCRT toolchain (example)
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\Users\Administrator\llvm-mingw-ucrt\bin;C:\Windows\System32;C:\Windows"
cargo test --workspace
cargo run -p echo-app
```

Optional Last.fm credentials can be entered in **Settings -> Last.fm Scrobbling** or via `ECHO_LASTFM_API_KEY` and `ECHO_LASTFM_API_SECRET` environment variables.
