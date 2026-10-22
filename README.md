# Niri Keymap Help

A GTK4 desktop app for browsing and searching the bindings in your Niri config.

- Search stays at the top and keeps keyboard focus while the window is active.
- Search matches keys, descriptions, and commands (including paths and arguments),
  ignoring case. Every word must match somewhere across these fields:
  `mod terminal kitty` can match `Mod+T`, “Open a Terminal”, and `spawn "kitty"`.
  Search `spawn` to find both `spawn` and `spawn-sh` bindings.
- By default bindings are grouped by their `Group: action` description prefix
  (e.g. `Focus: column left` lands under FOCUS as “column left”). Groups appear
  in the order the config first mentions them; bindings without a prefix go to
  OTHER, and undocumented ones to NO DESCRIPTION at the end.
- The sort menu (or Ctrl+S) switches between Grouped, Config order, Key, and
  Description.
- Bindings without descriptions have amber keys, a colored edge, and a warning label.
- Descriptions come from `hotkey-overlay-title`, falling back to an adjacent comment.
- Config changes reload automatically, including changes to included files.
- Hover over a row to see its action, description source, and config file/line.

## Build and run

Install Rust and GTK4 development libraries (GTK 4.8 or newer), plus `pkg-config`:

```sh
# Arch Linux
sudo pacman -S gtk4 pkgconf

# Debian / Ubuntu
sudo apt install libgtk-4-dev pkg-config build-essential

cargo run --release
```

The default path is `$XDG_CONFIG_HOME/niri/config.kdl`, falling back to
`$HOME/.config/niri/config.kdl`. On this machine that is
`/home/rods/.config/niri/config.kdl`.

```sh
cargo run --release -- --config /path/to/config.kdl
cargo run --release -- --sort key   # grouped | config | key | description
cargo run -- --check
cargo run -- --help
```

`--check` prints counts and parse errors without opening a window. This app
reads files on disk; it does not query the running compositor's loaded config
or perform a full Niri configuration validation.

To install the binary into `~/.local/bin` (requires [just](https://github.com/casey/just)):

```sh
just deploy
```

or with Cargo into `~/.cargo/bin`:

```sh
cargo install --path . --locked
```

An optional Niri shortcut (choose an unused key):

```kdl
binds {
    Mod+F1 hotkey-overlay-title="Search keymaps" {
        spawn "niri-keymap-help"
    }
}
```

Make sure the install directory is on Niri's `PATH`, or use an absolute binary path.
The app is single-instance: launching it again raises the existing window.
The app ID is `io.github.niri_keymap_help` if you want a Niri floating-window rule.
The app does not edit your config or execute any listed bindings.

## Keyboard controls

| Key | Action |
| --- | --- |
| Type | Filter keymaps, descriptions, and commands |
| Up / Down | Scroll the results |
| Page Up / Page Down | Scroll by a page |
| Escape | Clear the query; close if already empty |
| Ctrl+S | Cycle the sort order |
| Ctrl+A | Select the search text |

Focus stays in search when clicking or scrolling the list, and returns there
when the window becomes active. Other applications retain normal window focus.

## Description rules

1. Use a nonempty `hotkey-overlay-title`. Pango markup is converted to visible
   plain text so it can be searched and styled consistently.
2. If the title is absent, empty, or `null`, use the immediately preceding
   standalone `//` or `/* ... */` comment block.
3. Stop at blank lines, other code, separator headings, or commented-out binding
   examples. Comments are associated with one binding, never inherited by later
   bindings in a group. Inline trailing comments are not descriptions.
4. Otherwise show “No description”, with the action underneath for reference.

Comment association is a heuristic. Prefer an explicit title when a comment
describes several bindings. A `null` title hides a binding from Niri's overlay,
but keeps it visible here.

The KDL v1 parser understands strings, raw strings, multiline nodes, and `/-`
disabled nodes/properties. Includes resolve relative to the containing file
(absolute paths and `~/` are also supported). Missing optional includes are
shown as warnings and watched for creation. Later `binds` blocks override
earlier conflicting keys; modifier order and common aliases are normalized.
Separate shortcuts for the same action remain separate rows.

Files are checked on a background thread every 350 ms, including atomic editor
saves. Invalid edits display an error while retaining the last valid list and
current query. Includes are checked for cycles and excessive nesting.

## Development

```sh
just check   # cargo fmt --check, clippy -D warnings, cargo test
just build   # release build
```

Parsing, search, and reload tests run without a display. UI checks can be run
on an isolated X server:

```sh
xvfb-run -a env GDK_BACKEND=x11 cargo test --bin niri-keymap-help -- --ignored
```

Code is split into `config.rs` (KDL and descriptions), `search.rs`, `sort.rs`
(grouping and sort orders), `watch.rs`
(reload worker), `ui.rs` (GTK), and `style.css`. GTK 0.10 is used with GTK 4.8
APIs so a newer system GTK release is not required.
