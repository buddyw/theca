
	#  _   _                    
	# | |_| |__   ___  ___ __ _
	# | __|  _ \ / _ \/ __/ _` |
	# | |_| | | |  __/ (_| (_| |
	#  \__|_| |_|\___|\___\__,_|
	#

![example usage of theca](screenshots/main.png)

A simple, fully featured, command line note taking tool written in [*Rust*](http://www.rust-lang.org/). 

## Features

* **Secure by Design**: Uses **XChaCha20-Poly1305** for authenticated encryption and **Argon2id** for robust password hashing.
* **Modern Format**: Stores profiles in **YAML** for readability and editability.
* **Encrypted Profiles**: Full support for encrypted profiles with Base64 encoding for safe storage.
* **Multiple Profiles**: Segregate your notes into different profiles (e.g., `work`, `personal`).
* **Search**: Powerful keyword and regex search capabilities.
* **Editors**: Drop into your favorite `$EDITOR` to write complex notes.
* **Cross-Platform**: Works on Linux, macOS, and Windows.

## Contents

- [Installation](#installation)
- [Usage](#usage)
	- [First run](#first-run)
	- [Adding notes](#adding-notes)
	- [Listing notes](#listing-notes)
	- [Viewing notes](#viewing-notes)
	- [Editing notes](#editing-notes)
	- [Deleting notes](#deleting-notes)
    - [Searching](#searching-notes)
	- [Encrypted profiles](#encrypted-profiles)
- [Modernization](#modernization-v20)
- [License](#license)

## Installation

### From Source

Ensure you have [Rust and Cargo installed](https://rustup.rs/).

```bash
git clone https://github.com/buddyw/theca.git
cd theca
cargo install --path .
```

## Usage

```
theca 2.0.0
a simple, fully featured, command line note taking tool

Usage:
    theca [options] [command]

Commands:
    add               Add a new note
    edit              Edit an existing note
    del               Delete a note
    list-profiles     List profiles
    new-profile       Create a new profile
    transfer          Transfer a note to another profile
    encrypt-profile   Encrypt the current profile
    decrypt-profile   Decrypt the current profile
    search            Search notes
    info              Show profile info
    clear             Clear all notes
    list              List notes (default if no command)
    help              Print this message or the help of the given subcommand(s)

Options:
    -p, --profile <PROFILE>                Profile to use [default: default]
        --profile-folder <PROFILE_FOLDER>  Profile folder to use [env: THECA_PROFILE_FOLDER=]
    -k, --key <KEY>                        Encryption key [env: THECA_KEY=]
        --encrypted                        Encrypted profile (flag)
    -y, --yes                              Do not ask for confirmation
    -h, --help                             Print help
    -V, --version                          Print version
```

### First run

```bash
theca new-profile default
```

This creates a `default.yaml` profile in `~/.theca/` (or `$THECA_PROFILE_FOLDER`).

### Adding notes

```bash
theca add "My First Note"
theca add "Meeting Notes" --status urgent --body "Discussed roadmap..."
theca add "Complex Note" --editor
```

### Listing notes

```bash
theca list
theca list --datesort --reverse --limit 5
```

### Viewing notes

You can view a single note by providing its ID:

```bash
theca 1
```

### Editing notes

```bash
theca edit 1 --title "Updated Title" --status started
theca edit 1 --editor
```

### Deleting notes

```bash
theca del 1
theca del 2 3 5
```

### Searching notes

```bash
theca search "rust"
theca search "TODO.*" --regex
theca search "important" --search-body
```

### Encrypted Profiles

Theca v2.0 uses **XChaCha20-Poly1305** for encryption.

**Create an encrypted profile:**

```bash
theca new-profile secrets --encrypted
# You will be prompted for a password
```

**Access an encrypted profile:**

You must provide the `--encrypted` flag and the key (via flag, env var, or prompt).

```bash
theca --profile secrets --encrypted list
theca --profile secrets --encrypted --key "mysecretdrowssap" add "Bank Code" "1234"
```

**Encrypt an existing profile:**

```bash
theca --profile default encrypt-profile
```

**Decrypt a profile:**

```bash
theca --profile secrets --encrypted decrypt-profile
```

## Modernization (v2.0)

Version 2.0 represents a major overhaul of the Theca codebase:

*   **Cryptography**:
    *   **Algorithm**: Switched from AES-256-CBC to **XChaCha20-Poly1305**. This provides authenticated encryption and a larger nonce to safely prevent reuse vulnerabilities.
    *   **KDF**: Switched from PBKDF2 to **Argon2id** for state-of-the-art password hashing.
    *   **Storage**: Encrypted profiles are stored as Base64 encoded strings for better portability.

*   **Serialization**:
    *   Switched from JSON to **YAML**. All new profiles are created as `.yaml` files.
    *   *Note*: Legacy `.json` profiles are **not** compatible with v2.0. Users must migrate data manually or stick to v1.x if legacy support is required.

*   **Dependencies**:
    *   Argument parsing moved to `clap` v4.
    *   Terminal handling moved to `crossterm`.
    *   Removed unmaintained libraries like `rustc-serialize`.

## License

`theca` is licensed under the MIT license.
