# qist guide

qist means **quick list**. The Cargo package and executable are **`qs`**; the
configuration directory is **`qist`**. macOS and Linux are supported. macOS BSD
behavior is the primary compatibility target.

[README](README.md) · [Compatibility matrix](docs/COMPATIBILITY.md) ·
[Benchmark methodology](docs/BENCHMARKS.md) · [Demo production](docs/DEMO.md)

## Contents

- [Setup](#setup)
- [Everyday listings](#everyday-listings)
- [Output modes](#output-modes)
- [Selection and traversal](#selection-and-traversal)
- [Sorting and timestamps](#sorting-and-timestamps)
- [Metadata and macOS options](#metadata-and-macos-options)
- [Colors, icons, and links](#colors-icons-and-links)
- [Configuration](#configuration)
- [Scripting and JSON](#scripting-and-json)
- [Option reference](#option-reference)
- [Environment](#environment)
- [Troubleshooting](#troubleshooting)
- [Updating and uninstalling](#updating-and-uninstalling)
- [Development](#development)

## Setup

### Prerequisites

Use Rust 1.88 or newer with Cargo, a C linker, Make, and Python 3. Install Rust
through [rustup](https://rust-lang.org/tools/install/). The installed `qs` binary
has no Python or Rust runtime requirement.

On macOS, Apple's Command Line Tools provide the linker and Make:

```sh
xcode-select --install
rustc --version
cargo --version
python3 --version
```

Run `xcode-select --install` only if the tools are missing. On Linux, install your
distribution's C build toolchain, Make, and Python 3, then install Rust. Native
Windows is unsupported.

Download or clone this repository, then open a shell in the folder containing
`Cargo.toml` and `Makefile`. There are no packaged releases or Homebrew formula
assumed by these instructions.

### macOS and zsh

```sh
make install
source ~/.zshrc
command -v qs
qs --version
```

The default destination is `~/.softwares/bin/qs`. The installer writes a marked
PATH block to `~/.zshrc`. Before editing an existing file, it creates a timestamped
`.zshrc.qs-backup-*` backup. It preserves surrounding contents, permissions, and
symlinked shell configuration. Repeating installation updates the binary without
duplicating PATH entries or creating unnecessary backups.

Customize both destinations if needed:

```sh
make install PREFIX="$HOME/.local" ZSHRC="$HOME/.config/zsh/.zshrc"
```

`PREFIX` is the parent of `bin`; `ZSHRC` selects the shell file. If you set
`ZDOTDIR`, point `ZSHRC` at the `.zshrc` your shell actually reads.

### Cargo, Linux, and other shells

To install without editing any shell configuration:

```sh
cargo install --path . --locked
```

Cargo normally places the executable in `~/.cargo/bin`. Ensure that directory is
on your shell's PATH. Alternatively, build and copy it yourself:

```sh
make build
mkdir -p "$HOME/.softwares/bin"
install -m 755 target/release/qs "$HOME/.softwares/bin/qs"
```

For bash, add `export PATH="$HOME/.softwares/bin:$PATH"` to the startup file your
terminal reads. For fish, use `fish_add_path "$HOME/.softwares/bin"`. The Make
installer specifically manages a zsh file; use these manual paths for other shells.

### Try without installing

```sh
cargo run --release -- --table -h
./target/release/qs --tree --depth=2 src
```

`--` separates Cargo options from `qs` options. Release builds are the intended
choice for daily use and performance measurements.

## Everyday listings

```sh
qs                               # current directory
qs src docs                      # multiple paths, with headings as needed
qs -lah                          # long, all entries, human-readable sizes
qs -A                            # hidden entries, excluding . and ..
qs -lt                           # most recently modified first
qs -lSh                          # largest sizes first
qs -lShr                         # smallest sizes first
qs -F                            # directories /, executable files *, links @
qs --table -h src
qs --tree --depth=2 src
```

On macOS, parsing stops at the first operand: put flags **before paths**.
Use `--` before a filename beginning with `-`, for example `qs -l -- -report`.
Options can be bundled, and long options accept `--name=value` or a separate
required value. Optional arguments to `--color`, `--hyperlink`, and `--classify`
use `=`: `qs --color=always src`.

By default, names sort according to the current locale. A terminal receives
columns and automatic colors; redirected output receives one raw name per line
and normally no color. A configured default output mode changes this behavior.

## Output modes

| Mode | Select with | Purpose |
| --- | --- | --- |
| Automatic | `--output=auto` | Columns on a terminal, lines in a pipe |
| One per line | `-1` / `--output=single` | Conventional names-only output |
| Columns down | `-C` / `--output=columns` | Fill down, then across |
| Columns across | `-x` / `--output=across` | Fill across, then down |
| Comma stream | `-m` / `--output=commas` | Comma-separated terminal listing |
| Long | `-l` / `--output=long` | Traditional metadata columns |
| Table | `--table` | Labeled, aligned metadata table |
| Tree | `--tree` | Recursive tree with counts |
| JSON | `--json` | One structured document |

`--format=MODE` and `--output=MODE` are synonyms. Common format aliases
`single-column`, `vertical`, `horizontal`, and `verbose` are also accepted.

### Table

```sh
qs --table -h
qs -n --table -h src       # numeric IDs, then select the table layout
qs --table -h --icons
```

The table includes permissions, links, owner, group, size, modified time, and
name. `-i`, `-s`, `-O`, and context options add their associated fields. Width
calculations exclude ANSI/OSC escapes. Table and tree escape filename control
characters by default. `-n` selects long output as in `ls`; place `--table`
after it when you want a table with numeric IDs.

### Tree

```sh
qs --tree --depth=2
qs --tree -a --depth=3 src
qs --tree -L --one-file-system --depth=3 src
```

Tree mode recurses without `-R`. Depth is measured from each root: `--depth=0`
shows the root only, and `--depth=1` includes its immediate children. A depth
limit also applies to recursive classic listings and JSON. The maximum accepted
value is 4096. Ancestor `(device, inode)` identities detect symlink cycles;
independent paths to the same directory remain allowed. `.` and `..` are never
traversed. Counts summarize entries actually visited within the limit.

### JSON

```sh
qs --json src
qs --json -R --depth=3 src
qs --json -d src           # describe the directory itself
```

JSON reads a directory's immediate entries by default; `-R` enables deeper
traversal. It always emits one document with a schema version and error list.
See [Scripting and JSON](#scripting-and-json).

## Selection and traversal

| Option | Behavior |
| --- | --- |
| `-a`, `--all` | Include hidden names, `.` and `..` |
| `-A`, `--almost-all` | Include hidden names, excluding dot entries |
| `-d`, `--directory` | Describe directory operands themselves |
| `-R`, `--recursive` | Recurse in classic and JSON modes |
| `--depth=N` | Limit traversal in recursive, tree, and JSON output |
| `--ignore=GLOB` | Exclude matching names; repeatable |
| `--hide=GLOB` | Hide matches unless `-a` or `-A` is active |
| `--ignore-backups` | Hide names ending in `~` |
| `--one-file-system` | Avoid crossing filesystem boundaries during traversal |

Quote globs so your shell does not expand them first:

```sh
qs --ignore='*.tmp' --ignore='*.log' src
qs --hide='.*' src
```

Following links is explicit:

- `-P`: describe argument symlinks themselves.
- `-H`: follow directory symlink operands in BSD mode; preserve file-link metadata.
- `-L`: follow all links, including those found during traversal.

The default follows the host's conventional operand-link rules; use explicit
flags when the distinction matters. `--dereference`,
`--dereference-command-line`, and `--dereference-command-line-symlink-to-dir`
provide long spellings. On macOS, root users see hidden entries implicitly,
as with `ls`; `-I` disables that implicit behavior unless `-a`/`-A` was selected.

## Sorting and timestamps

```sh
qs --sort=version                         # file2 before file10
qs --sort=size -r                         # smallest first
qs --sort=extension                       # group by suffix
qs --sort=none                           # enumeration order
qs --group-directories-first
qs -lt --time=birth                       # creation time where available
qs -l --time-style=long-iso
qs -l -D '%Y-%m-%d %H:%M:%S'              # macOS custom format
```

Sort keys are `name`, `none`, `size`, `time`, `extension`, `version`, and `width`.
`-S` and `-t` select size and time. `-r` reverses the ordering. Directory grouping
is optional. In BSD mode, `-f` also includes hidden entries and keeps sorting
disabled even if a later `-t` or `-S` occurs.

Timestamp keys are `mtime`/`modification`, `atime`/`access`/`use`,
`ctime`/`status`, and `birth`/`creation`. Short flags `-u`, `-c`, and macOS `-U`
select access, change, and birth timestamps. In BSD mode these alone do not
implicitly enable time sorting: add `-t`.

Styles are `locale`, `iso`, `long-iso`, `full-iso`, `bsd-full`, or a custom
`+strftime-format`. `--full-time` selects long output and `full-iso`. macOS `-T`
shows seconds and year; `-D FORMAT` takes precedence over `-T`. macOS `-y` makes
timestamp and alphabetical ties sort in the same direction. Birth time can be
unavailable on Linux/filesystems without support; JSON represents it as `null`.

## Metadata and macOS options

```sh
qs -l@eO                                 # xattrs, ACLs, BSD flags
qs -ln                                   # numeric owner/group IDs
qs -lis                                  # inode and allocated blocks
qs -lh                                   # base-1024 human sizes
qs -l --si                               # base-1000 human sizes
qs -l --block-size=MiB                    # explicit display units
qs -l,                                   # locale thousands separators
```

| Option | Meaning |
| --- | --- |
| `-@` | Extended attribute names and sizes under long entries |
| `-e` | ACL entries under long entries |
| `-O` | BSD file flags in long output |
| `-%` | Mark dataless entries; avoid traversing dataless directories |
| `-W` | Include whiteouts when the filesystem exposes them |
| `-n` | Numeric UID/GID; selects long output |
| `-g` | Omit owner in BSD long output |
| `-o` | Long output without group |
| `-i`, `-s` | Inode number and allocated-block count |
| `-k` | 1024-byte blocks; disables human formatting in BSD mode |
| `-,` | Locale-aware digit grouping |
| `--author` | Author column (UID identity on supported Unix hosts) |
| `--context`, `-Z` | Security context where available |

BSD allocated-block units default to 512 bytes. File sizes and allocated blocks
are different quantities, especially for sparse files. Flags, xattrs, and ACLs
come from native APIs. On Linux, `-e` uses POSIX ACL xattrs, SELinux context comes
from its xattr, and BSD flags have no equivalent in this implementation.
Dataless/whiteout support is implemented but lacks live local fixtures; see the
[compatibility limits](docs/COMPATIBILITY.md#validation-and-limits).

## Colors, icons, and links

```sh
qs --color=auto
qs --table -h --color=always
NO_COLOR=1 qs
qs --icons
qs --hyperlink=auto
```

`auto` requires a suitable terminal and respects `NO_COLOR` and `TERM=dumb`.
`CLICOLOR_FORCE` can request colors in redirected output when automatic color
is otherwise allowed. Explicit `--color=always` overrides `NO_COLOR`/dumb-terminal
suppression. `--color=never` disables it. JSON always remains ANSI-free.

The default colors use your terminal's ANSI palette rather than fixed RGB.
Configuration accepts numeric SGR parameters: `1;34` is bold blue, `36` cyan,
`2` dim, and the empty string uses the default foreground. Indexed colors
(`38;5;75`) and true color (`38;2;100;180;255`) are accepted when your terminal
supports them.

In BSD mode, `LSCOLORS` wins when both it and `LS_COLORS` are set. In GNU mode,
`LS_COLORS` wins. File-type and supported numeric suffix rules override configured
name colors; table headers, dates, and sizes retain their separate styles.
Unsupported nonnumeric escape framing is ignored.

Icons use ordinary Unicode symbols and need no Nerd Font. OSC 8 hyperlinks are
off by default; `--hyperlink=auto` enables them for terminals, and `always` forces
them. Terminal support determines whether links are clickable.

## Configuration

```sh
qs --init-config
qs --print-config
qs --config=./my-qist.toml --table
qs --no-config -1
```

The config is `$XDG_CONFIG_HOME/qist/config.toml` when that variable is nonempty,
otherwise `$HOME/.config/qist/config.toml`. A missing implicit default file is
fine. A missing explicit `--config` path is an error. `--init-config` creates
parent directories and refuses to overwrite an existing file.

All keys are optional. Unknown keys and malformed settings produce errors.
The complete shipped example is [config.example.toml](config.example.toml):

```toml
default_output = "auto"
color = "auto"
icons = false
human_readable = false
group_directories_first = false
# tree_depth = 3

[colors]
directory = "1;34"
symlink = "36"
executable = "32"
file = ""
broken_symlink = "1;31"
header = "1"
muted = "2"
size = "33"
date = "2"
```

| Key | Default | Accepted values |
| --- | --- | --- |
| `default_output` | `"auto"` | `auto`, `single`, `columns`, `across`, `commas`, `long`, `json`, `table`, `tree` |
| `color` | `"auto"` | `auto`, `always`, `never` |
| `icons` | `false` | Boolean |
| `human_readable` | `false` | Boolean |
| `group_directories_first` | `false` | Boolean |
| `tree_depth` | Unlimited | Nonnegative integer, at most 4096 |
| `[colors]` values | See above | SGR numeric parameters separated by semicolons |

CLI options override the corresponding settings. A command-line output selector
replaces `default_output`; `--no-icons` overrides `icons`. `--no-config` uses
built-in defaults. `--print-config` prints loaded TOML **before CLI overrides**;
it is not an effective-options dump. There is no environment-variable expansion
inside TOML strings.

For a tree by default, set `default_output = "tree"` and `tree_depth = 2`. To keep
conventional behavior for a script regardless of your personal defaults, use
`qs --no-config -1 --color=never` or an explicit JSON command.

## Scripting and JSON

### Filenames

Newlines, spaces, and non-UTF-8 bytes can occur in Unix filenames. Avoid splitting
ordinary `ls`-style output on whitespace. NUL output preserves name bytes:

```sh
qs --no-config --zero src | xargs -0 -I '{}' printf '<%s>\n' '{}'
```

This emits listed **names**, not a `find`-style list of fully joined paths. Use
JSON `path_bytes` when a script needs exact paths. `--zero` disables colors,
hyperlinks, and quoting; it is incompatible with `--dired`.

For terminal display, `-q` replaces unprintable characters, macOS `-b` escapes
nonprinting bytes, and macOS `-B` uses octal escapes. `--quoting-style` supports
`literal`, `shell`, `shell-always`, `shell-escape`, `shell-escape-always`, `c`,
`c-maybe`, `escape`, `locale`, and `clocale`. Raw modes (`-v`/`-w` on macOS)
should be chosen deliberately for unusual filenames.

### JSON envelope

```sh
qs --no-config --json -R --depth=2 src > listing.json
python3 -m json.tool listing.json
```

The envelope is `{ "schema_version": 1, "roots": [...], "errors": [...] }`.
Each root is an entry. Its `entries` array contains children when read. The
following is a **projection** of selected fields, not the entire entry schema:

```json
{
  "schema_version": 1,
  "roots": [{"name": "src", "type": "directory", "entries": []}],
  "errors": []
}
```

| Entry field | Meaning |
| --- | --- |
| `name`, `path` | Display strings; invalid UTF-8 is replaced |
| `name_bytes`, `path_bytes` | Exact Unix bytes as integer arrays |
| `type` | `file`, `directory`, `symlink`, `fifo`, `socket`, `block_device`, `char_device`, `whiteout`, `unknown` |
| `inode` | Entry inode |
| `permissions` | Formatted Unix permission string |
| `metadata` | Object below, or `null` |
| `symlink_target`, `symlink_target_bytes` | Link text and exact bytes, or `null` |
| `broken_symlink` | Whether the link target could not be resolved |
| `error` | Entry metadata error, or `null` |
| `extended_attributes` | Array of `{name, name_bytes, size}`; requested with `-@` |
| `acl` | Native formatted ACL text, or `null`; requested with `-e` |
| `security_context` | Context text, or `null`; requested with `--context` |
| `entries` | Present for a directory that was read |
| `cycle` | `true` when traversal detects an ancestor cycle |
| `read_error` | Present when reading a directory failed |

Metadata fields are `inode`, `device`, `mode`, `links`, `uid`, `gid`, `size`,
`blocks`, `rdev`, `flags`, `modified`, `accessed`, `changed`, and `created`.
`size` is bytes; `blocks` counts allocated 512-byte blocks. `mode` includes file
type and permission bits. `device` and `rdev` are raw device IDs. Timestamps use
`{ "seconds": ..., "nanoseconds": ... }` since the Unix epoch. `created` can be
`null`. JSON uses numeric data independent of human-readable display settings.

For a simple dependency-free consumer:

```sh
qs --no-config --json src | python3 -c '
import json, sys
listing = json.load(sys.stdin)
for root in listing["roots"]:
    for entry in root.get("entries", []):
        print(entry["path"], entry["type"])
'
```

### Errors and exit status

Missing paths, unreadable directories, cycles, and locale collation failures
produce stderr diagnostics and a nonzero status while preserving usable results
for other operands. JSON retains a valid envelope and an `errors` array of
`{path, message}` objects for listing errors. Argument/config errors happen
before listing and do not produce a JSON envelope.

Success is 0. BSD listing/argument errors normally return 1; GNU serious operand
or argument errors return 2, while recoverable traversal errors can return 1.
Unexpected output/I/O failures can return 2. Check for **nonzero**, rather than
assuming every failure has the same code. A closed pipe follows normal Unix
SIGPIPE handling.

## Option reference

The sections above cover qist's own options and daily `ls` usage. The complete
[compatibility reference](docs/COMPATIBILITY.md) lists every supported macOS
short option, GNU long option, and conflict. `qs --help` is available locally.

Additional classic layout controls include `--width=N` and `--tabsize=N`.
Indicators are selected by `--indicator-style=none|slash|file-type|classify`,
`--classify[=WHEN]`, `--file-type`, or `-F`/`-p`. `--dired` adds Emacs offsets
for conventional listings. `--help` and `--version` work without loading config.
Long-option abbreviations are accepted only when unambiguous; full names are
recommended in scripts.

Conflicting macOS/GNU flags include `-B`, `-D`, `-G`, `-g`, `-I`, `-T`, `-U`,
`-v`, `-w`, and `-X`. For example, macOS `-U` selects birth time, while GNU `-U`
means unsorted. Prefer explicit long options or select a dialect first:

```sh
qs --compat=bsd -l@eO src
qs --compat=gnu -v --group-directories-first src
```

Parsing an option does not imply every GNU edge case is identical. Historical
macOS compatibility modes are not emulated. Consult the documented boundaries
before depending on exact output in portable scripts.

## Environment

| Variable | Effect |
| --- | --- |
| `HOME`, `XDG_CONFIG_HOME` | Default config discovery |
| `TERM`, `NO_COLOR`, `CLICOLOR_FORCE` | Automatic color eligibility |
| `LSCOLORS`, `LS_COLORS` | BSD/GNU name-color rules and precedence |
| `LC_ALL`, `LC_COLLATE`, `LC_CTYPE`, `LC_TIME`, `LC_NUMERIC`, `LANG` | Native collation, names, dates, numeric formatting |
| `COLUMNS`, `TABSIZE` | Layout width and tab size |
| `LS_COLWIDTHS` | BSD minimum field widths |
| `BLOCKSIZE` | BSD block size, interpreted with native `getbsize` on macOS |
| `LS_SAMESORT` | Same-direction BSD timestamp ties |
| `TIME_STYLE`, `QUOTING_STYLE` | GNU mode defaults; ignored in BSD mode |
| `LS_BLOCK_SIZE`, `BLOCK_SIZE` | GNU block-size defaults; ignored in BSD mode |
| `POSIXLY_CORRECT` | GNU parsing termination and default block behavior |
| `QIST_NO_BULK=1` | Diagnostic macOS fallback to individual metadata calls |

`CLICOLOR` does not need to be set: qist already defaults to automatic color.
`COMMAND_MODE=legacy` and `COMPAT_MODE` are not implemented. Use `LC_ALL=C` for
bytewise ordering in reproducible comparisons; use your normal locale for
language-aware sorting.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `qs: command not found` | Open a new terminal or `source ~/.zshrc`; run `command -v qs`; check custom `ZDOTDIR`/`ZSHRC` |
| Old binary runs | `type -a qs` shows aliases/functions and PATH order; rebuild and reinstall |
| No colors | Check `TERM`, `NO_COLOR`, redirection; try `--color=always` |
| Colors ignore `[colors]` | Existing `LSCOLORS` / `LS_COLORS` may override file-name colors |
| Unexpected output mode | Check `--print-config`; retry with `--no-config` |
| Flags become filenames | Put every option before the first operand in BSD mode |
| JSON consumer fails | Keep stdout/stderr separate; inspect exit code; argument errors do not emit JSON |
| A directory changes mid-listing | Filesystems are live; output is not an atomic snapshot |
| Cloud/network behavior differs | See live-fixture limits; compare `/bin/ls`; report filesystem and flags |
| Installer rejects markers | Repair duplicate/incomplete `# >>> qs PATH >>>` blocks before retrying |

For a reproducible bug report include `qs --version`, OS/architecture, filesystem,
exact flags, relevant environment/config, expected `/bin/ls` output, and actual
output. Remove private filenames from the report where needed.

## Updating and uninstalling

Update the checkout, then rerun `make install`. Existing config is preserved.
For Cargo installations, rerun `cargo install --path . --locked --force`.

To uninstall the default Make installation, remove `~/.softwares/bin/qs` and
remove only the block between `# >>> qs PATH >>>` and `# <<< qs PATH <<<` in
`~/.zshrc`. Keep the directory on PATH if other applications use it. You may
retain `~/.config/qist/config.toml` for a future reinstall. For Cargo use
`cargo uninstall qs`. There is no Make uninstall target.

## Development

```sh
make check              # rustfmt and strict clippy
make test               # Rust and installer tests
make build              # optimized binary
make parity             # macOS fixture comparisons
make benchmark          # warm-cache 20,000-file comparison
```

Override `CARGO`, `PYTHON`, `CARGO_TARGET_DIR`, `PREFIX`, and `ZSHRC` as needed.
`make parity` writes `target/macos-parity.json` by default; `make benchmark`
writes `target/benchmark.json`. Benchmark flags can be supplied with
`BENCHMARK_ARGS='--entries 100 --runs 31'`.

CI is configured for macOS and Linux. Local macOS testing does not establish
Linux runtime behavior. Expand differential fixtures for compatibility changes.
For Linux compile checks from macOS:

```sh
rustup target add x86_64-unknown-linux-gnu
cargo check --target x86_64-unknown-linux-gnu --all-targets --locked
```

The [benchmark guide](docs/BENCHMARKS.md) explains measurement limits. The
[demo guide](docs/DEMO.md) explains media regeneration, capture provenance, and
GitHub upload. `make clean` removes Cargo build products, including files under
`target`; committed docs/media remain.
