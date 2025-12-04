# rustmail

A beautiful, fast, terminal-based email client with vim keybindings. Built in Rust.

![rustmail](https://img.shields.io/badge/rust-1.70+-orange.svg)
![license](https://img.shields.io/badge/license-MIT-blue.svg)

## Features

- **Vim-first** — Navigate with `j/k`, open with `l`, go back with `h`
- **Fast** — Native Rust performance, instant response
- **Gmail OAuth** — Secure authentication, no password storage
- **Minimal UI** — Clean, distraction-free interface
- **Full workflow** — Read, compose, reply, forward, archive, delete

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
git clone https://github.com/labfus-batlux/rustmail.git
cd rustmail
cargo build --release
./target/release/rustmail
```

## Setup

### 1. Create Google OAuth Credentials

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create a new project
3. Enable the **Gmail API** (APIs & Services → Library)
4. Create OAuth credentials (APIs & Services → Credentials → Create Credentials → OAuth client ID)
5. Select "Desktop app" as application type
6. Note your **Client ID** and **Client Secret**

### 2. Configure rustmail

Create `~/.config/rustmail/config.toml`:

```toml
email = "your.email@gmail.com"
client_id = "your-client-id.apps.googleusercontent.com"
client_secret = "your-client-secret"
```

### 3. Run

```bash
rustmail
```

On first run, a browser window will open for OAuth authentication.

## Keybindings

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` | Move down / up |
| `gg` | Go to first |
| `G` | Go to last |
| `Enter` / `l` | Open email |
| `h` / `q` / `Esc` | Go back |

### Folders

| Key | Action |
|-----|--------|
| `gi` | Go to Inbox |
| `gt` | Go to Sent |
| `gd` | Go to Drafts |
| `ge` | Go to Trash |
| `ga` | Go to Archive |

### Actions

| Key | Action |
|-----|--------|
| `c` | Compose new email |
| `r` | Reply |
| `a` | Reply all |
| `f` | Forward |
| `e` | Archive |
| `d` | Delete |
| `s` | Star / unstar |
| `R` | Refresh |

### Search & Commands

| Key | Action |
|-----|--------|
| `/` | Search emails |
| `:` | Command palette |
| `?` | Help |

### Compose (vim-style)

| Key | Action |
|-----|--------|
| `i` / `a` | Enter insert mode |
| `Esc` | Normal mode |
| `h/j/k/l` | Navigate (normal mode) |
| `w` / `b` | Word forward / back |
| `Tab` | Next field |
| `Ctrl+s` | Send |

## Commands

Open with `:` then type:

- `:inbox` — Go to inbox
- `:sent` — Go to sent
- `:drafts` — Go to drafts  
- `:trash` — Go to trash
- `:archive` — Go to archive
- `:refresh` — Refresh emails
- `:quit` — Quit

## License

MIT
