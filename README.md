# rustium

> **Proof of concept.** rustium was built out of experimental interest and is **not actively maintained**. It was vibe-coded with the help of Claude, Anthropic's AI assistant: the code was largely written by AI and has not had a thorough human review. Use it at your own risk; issues and pull requests may go unanswered.

rustium is a proof-of-concept remake of [Ferdium](https://github.com/ferdium/ferdium-app) in Rust. It rebuilds Ferdium's core idea, several web messengers side by side in one window, from scratch with [Tauri 2](https://v2.tauri.app). It contains no Ferdium code and is not affiliated with the Ferdium project.

The experiment: can a Rust shell on the system webview (WKWebView on macOS, WebView2 on Windows, WebKitGTK on Linux) run several WhatsApp Web accounts, all active so notifications keep arriving, with less memory than Electron-based Ferdium? The web apps themselves still use most of the memory, and WebView2 on Windows is based on Chromium, so the shell alone can only save part of it.

rustium was built with WhatsApp Web in mind, but it can show any application that has a web app: a service is just a name and an `https://` URL. Unread badges work for pages that put the count in their title (like `(3) WhatsApp`), and notifications for pages that use the standard Web Notification API.

## Features

- Each service is a name and a URL, shown in its own isolated webview with separate cookies and storage. Several accounts of the same messenger can be logged in at once.
- A sidebar on the left switches between services and shows unread counts, read from the page title (for example `(3) WhatsApp`).
- Web notifications are forwarded to native OS notifications, prefixed with the service name.
- Services stay loaded while hidden, so they can keep receiving messages.
- A tray icon keeps rustium running when the window is closed.

## Usage

On first start rustium opens one WhatsApp Web service. Click `+` at the bottom of the sidebar to add a service (a name and an `https://` URL; the URL defaults to WhatsApp Web) or to remove one. Removing a service also deletes its stored login data. Services are saved in `services.json` in the app's config directory.

Requires macOS 14 or later (isolated storage per service needs it), Windows 10 or later with WebView2, or a Linux desktop with WebKitGTK 4.1.

## Status

Proof of concept, not actively maintained. Known limits:

- Clicking a notification does not focus the window or switch to the service yet.
- Microphone and camera access are not handled yet, so voice messages and taking photos do not work. WhatsApp Web also shows a banner recommending another browser, because rustium presents itself as Safari. Downloads go to the Downloads folder.
- On Linux the sidebar does not get its fixed width yet (child webviews ignore their position there), so the layout is broken on Linux.
- On Windows, each service currently gets its own WebView2 browser process, because Tauri does not expose WebView2 profiles yet. RAM use on Windows may therefore not improve compared to Ferdium.
- On Linux and Windows, rustium cannot disable background throttling of hidden webviews. It still has to be verified that notifications from hidden services arrive reliably there.
- Prebuilt bundles are not code-signed or notarized yet, so macOS Gatekeeper and Windows SmartScreen warn before the first start.

## First measurement

macOS 26, the same WhatsApp account freshly logged in to all three apps and left idle for a few minutes, memory as Activity Monitor counts it (physical footprint, summed over each app's processes):

| App | Total | WhatsApp page | The app itself |
|---|---|---|---|
| WhatsApp (official native app) | 215 MB | (no web page) | 190 MB |
| Ferdium | 1,140 MB | 671 MB (Chromium) | about 470 MB |
| rustium 0.1.0 | 1,408 MB | 1,217 MB (WebKit) | about 190 MB |

The Rust shell saves about 280 MB because it ships no browser engine of its own. But WebKit needs about 1.8 times as much memory as Chromium for the same WhatsApp Web page, so on macOS rustium ends up about 270 MB heavier than Ferdium. A Windows measurement (where WebView2 is Chromium) is still to come; `scripts/measure-memory.ps1` measures all three apps there.

## Build from source

There is no npm or frontend build step. The UI in `ui/` is plain HTML that Tauri embeds at compile time.

1. Install Rust with [rustup](https://rustup.rs).

2. On Linux, install the WebKitGTK and tray dependencies. On Debian or Ubuntu:

   ```sh
   sudo apt update
   sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
     libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
   ```

   For other distributions, and for the macOS (Xcode Command Line Tools) and Windows (Microsoft C++ Build Tools, WebView2) prerequisites, see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

3. Run a development build:

   ```sh
   git clone https://github.com/PrevosHD/rustium.git
   cd rustium
   cargo run
   ```

4. Build installable bundles (`.dmg` on macOS, `.msi` and `.exe` on Windows, `.deb`, `.rpm` and `.AppImage` on Linux) with the Tauri CLI:

   ```sh
   cargo install tauri-cli --version "^2" --locked
   cargo tauri build
   ```

   The bundles are written to `target/release/bundle/`.

## License

MIT
