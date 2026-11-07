# Bande platform layer for Gecko

Replace Firefox's GTK layer with a Bande one. Numbers are measured on Debian `firefox-esr` 153.6.0esr unless marked estimate.

## What the user gets
- A browser with no GTK: the default install loses about 132 MB and 78 packages (computed; to be verified on a VM in the last stage).
- Windows, popups, clipboard, drag, input methods, file dialogs and printing behave like every other Bande app.
- Firefox's engine, tab bar, address bar, Sync, add-ons and real uBlock Origin are unchanged. Firefox updates keep flowing from Debian.

## What "core" means
There is no pre-platform core to download. Layout, DOM, SpiderMonkey and WebRender sit above `nsIWidget`, and every platform is a backend under `widget/`:

| Backend | Files | Lines |
|---|---|---|
| gtk | 191 | 76,216 |
| windows | 175 | 75,880 |
| cocoa | 122 | 43,380 |
| android | 79 | 18,153 |
| uikit | 30 | 4,160 |
| headless | 17 | 1,625 |

So "strip to the core and add Bande libraries" means a new backend. Nothing in the engine has to be removed for it.

## Design
1. **Configure value `cairo-bande`.** `MOZ_WIDGET_TOOLKIT` stays `gtk` and the macro `MOZ_WIDGET_GTK` keeps meaning "Linux desktop". A new `MOZ_BANDE` selects `widget/bande`.
   - `MOZ_WIDGET_GTK` and friends appear on 606 lines in 225 files outside `widget/gtk`. Most choose Linux behaviour: DMABuf, Wayland, EGL, sandbox, fontconfig. A new toolkit name would have to patch all of them on every Firefox update.
   - Only 50 files outside `widget/gtk` call GTK, GDK, GLib or D-Bus, and 13 of those are `accessible/atk`. Those are what we patch.
2. **`widget/bande`: new files, copied in by `prepare.sh`, not a patch.** A thin C++ `nsIWidget`, app shell and services over a Rust library. New files can't conflict with an update.
3. **Reuse the Wayland half of `widget/gtk` from its own directory.** `DMABuf*`, `WaylandBuffer`, `WaylandVsyncSource`, `WaylandSurface`, `nsWaylandDisplay` and the compositor widget have 0 to 34 GTK lines each. `widget/bande/moz.build` compiles them in place, so WebRender and video upstream work arrives with updates. The few GDK lines get a small patch.
4. **`libbande-gecko.so`: a Rust crate pinned to desktop like the apps**, reached through a small C ABI and `dlopen`ed by the parent process only. A toolkit bump rebuilds this library and not libxul. Content processes and their sandboxes don't load it.
5. **Toolkit additions land in desktop first:** a `Window` whose content someone else renders, and popups for that content.
6. **Window layout as today:** toplevel surface from the toolkit, Gecko's rendering on a subsurface. Client-side, app id `org.bande.Browser`, so the compositor's control well keeps working.

## GLib
Aim: none. Outside `widget/gtk`, it is used for:
- **Main loop:** replaced by the app shell polling the Wayland socket.
- **`widget/gtk` D-Bus and GSettings, portals, MPRIS, wake lock, remote, shell integration, UPower, geolocation:** moved to the Rust library over `zbus`, which the toolkit already links, or dropped where Bande has no such service.
- **Screen capture:** libwebrtc's portal code uses GIO in 11 files under `third_party/libwebrtc`. A replacement of those files is the one part that touches vendored code; decided at that stage.

## Accessibility
Screen readers must keep working: the GTK dependency isn't dropped from the package until this stage passes.
- `accessible/atk` is 4,986 lines over ATK, and `atk-bridge` turns ATK into AT-SPI. ATK and the bridge need GLib but not GTK.
- **Native AT-SPI over `zbus` (preferred):** AT-SPI is pull-based, as Gecko's accessible tree and its cross-process cache are, and it removes ATK and GLib.
- **AccessKit:** used by the toolkit, but it pushes node data. Gecko would have to push every text run of every page across processes. To be measured before rejecting it.
- **Keep ATK and its bridge:** the fallback if native AT-SPI can't match it. Keeps GLib.

## Why this over the alternatives
- **New toolkit name in the build system:** cleaner name, but a patch queue about 5 times larger (225 files against about 50, including the 13 ATK files) and the conflict risk the owner excluded.
- **Providing fake `gtk.h`/`gdk.h` over Bande:** no real separation, and it would chase GTK semantics forever.
- **Keeping GTK, GLib-free or hidden:** doesn't remove the dependency.

## Upstream churn
Lines added plus removed between ESRs:

| Directory | 128 → 140 | 140 → 153 |
|---|---|---|
| `widget/gtk` | 26,341 | 33,650 |
| `widget/cocoa` | 14,147 | 11,757 |
| `widget/uikit` | 730 | 1,431 |
| `widget/headless` | 241 | 246 |

- We stop carrying the need to follow GTK changes, but `widget/gtk`'s Wayland half keeps coming to us for free.
- The small backends are the fair guide to forced work: a few hundred to about 1,500 lines per yearly ESR (estimate for ours, about 10,000 lines of C++ and Rust).
- The interface can jump: `nsBaseWidget` was folded into `nsIWidget` between 140 and 153 (about 3,500 lines moved).
- ESR point releases rarely touch `widget/`. Not measured yet; to check with the first point release after this lands.

## Carried patches (estimate, to correct after the proof)
About 40 small patches' worth of hunks in about 40 existing files, about 500 changed lines, on top of about 10,000 lines of new files.

## Stages
1. This plan.
2. Proof: no libgtk in `readelf -d`, a Bande window rendering a real page, pointer and keyboard, under the real headless compositor on a build VM.
3. Then, each driven by real input: tabs and the combined row; popups and menus; clipboard and drag; input methods; fullscreen; HiDPI; file dialogs; printing; accessibility; theme and dark mode; screen capture.
4. Drop GTK from the package's dependencies; measure the default-install size on a VM.
