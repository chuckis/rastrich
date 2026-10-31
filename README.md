# rastrich

A terminal Nostr client for Linux, built with Rust, [Ratatui](https://ratatui.rs), and
[nostr-sdk](https://github.com/rust-nostr/nostr).

rastrich is a small read-and-post client: browse a timeline of the accounts you follow, open
threads and profiles, publish notes and replies, react, and manage relays. Events are cached
locally in SQLite, and your signing key never goes into the config file.

## Features

- Timeline filtered to your NIP-02 follow list plus your own notes, with live updates
- Threaded conversations (NIP-10), author profiles, and note reactions
- Compose notes and replies with a multi-line editor
- Three account modes: read-only, local nsec, or NIP-46 bunker
- Relays can be added, removed, and refreshed from the UI
- Local SQLite event cache and daily log files
- Secrets stored in the system keyring, not on disk in plain text

## Requirements

- Linux
- Rust 1.88 or newer
- A terminal
- A Secret Service keyring (GNOME Keyring, KWallet, etc.) if you use a local nsec or bunker
- SQLite development libraries, unless your build of `libsqlite3-sys` bundles them

## Build and run

```sh
cargo build --release
./target/release/rastrich
```

Or install it into your Cargo bin directory:

```sh
cargo install --path .
rastrich
```

For development:

```sh
cargo run
```

### Command-line options

```text
--config <PATH>    Path to config.toml
--data-dir <DIR>   Directory for the local event cache
```

## Signing in

On the login screen, press `Tab` / `Shift+Tab` to choose a mode, paste the key or URI, and press
`Enter`.

| Mode | Input | Can post |
| --- | --- | --- |
| read-only | `npub` or hex public key | No |
| local key | `nsec` | Yes |
| bunker | `bunker://` URI (NIP-46) | Yes |

The `nsec` is saved in the system keyring. The config file only stores the `npub` and mode, so
sessions can be restored on the next launch. For bunker accounts, both the bunker URI and the
generated app key are kept in the keyring.

## Keyboard shortcuts

### General

| Key | Action |
| --- | --- |
| `?` | Toggle help |
| `q` / `Ctrl+C` | Quit |
| `Esc` | Go back / close the current view |

### Timeline

| Key | Action |
| --- | --- |
| `j` / `k`, `↓` / `↑` | Move selection |
| `g` / `G`, `Home` / `End` | Jump to top / bottom |
| `Enter` | Open the selected note's thread |
| `c` | Compose a note |
| `r` | Reply to the selected note |
| `l` / `+` | React with `+` |
| `p` | Open the author's profile |
| `R` | Manage relays |
| `Ctrl+R` | Reload the timeline |

### Composer

| Key | Action |
| --- | --- |
| `Enter` / `Ctrl+S` | Publish the note |
| `Alt+Enter` / `Shift+Enter` | Insert a newline |
| `Esc` | Cancel |

`Ctrl+Enter` also publishes in most terminals, because they report it as a plain `Enter`.

### Threads and profiles

| Key | Action |
| --- | --- |
| `j` / `k`, `↓` / `↑` | Move selection |
| `g` / `G`, `Home` / `End` | Jump to top / bottom |
| `Enter` | Open the selected note's thread |
| `r` | Reply |
| `l` / `+` | React with `+` |
| `p` | Open the author's profile (threads) |
| `Esc` | Go back |

### Relays

| Key | Action |
| --- | --- |
| `a` | Add a relay (enter a `wss://` URL) |
| `d` / `Delete` | Remove the selected relay |
| `r` | Refresh relay status |
| `j` / `k`, `↓` / `↑` | Move selection |
| `Esc` | Close |

## Configuration and data

Default locations follow the XDG directories:

| Path | Purpose |
| --- | --- |
| `~/.config/rastrich/config.toml` | Account and relay configuration |
| `~/.local/share/rastrich/events.sqlite3` | Local event cache |
| `~/.local/state/rastrich/logs/` | Daily log files |

Use `--config` and `--data-dir` to override the config file and cache location. Logs always stay
in the state directory.

Example `config.toml`:

```toml
[account]
npub = "npub1..."
mode = "local" # read_only, local, or bunker

[[relays]]
url = "wss://relay.damus.io"
read = true
write = true

[[relays]]
url = "wss://nos.lol"
read = true
write = true

[[relays]]
url = "wss://relay.primal.net"
read = true
write = true
```

Defaults are created on first launch, and changes made in the relay manager are saved back to
`config.toml`.

## Notes and limitations

- The timeline is follow-list based: it shows notes from accounts in your NIP-02 contact list plus
  your own notes. If no contact list is available, only your own notes are shown.
- This is an MVP focused on reading and posting. Direct messages, notifications, media uploads,
  and wallet features are not implemented.
- Linux only.
- Debug builds are large; use `--release` for normal use.

## Development

```sh
cargo fmt --all
cargo test
cargo clippy --all-targets -- -D warnings
```
