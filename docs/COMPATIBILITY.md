# ls compatibility

qist's `qs` executable is implemented in Rust against Unix system libraries. It never delegates
listing or rendering to another program. The host dialect is the default;
`--compat=bsd` and `--compat=gnu` select short-option meanings explicitly.
macOS is the primary compatibility target; Linux support is maintained without
changing the host macOS behavior to match GNU ls.

## Shared options

`-1 -A -a -b -C -c -d -F -f -H -h -i -k -L -l -m -n -o -p -q -R -r -S -s -t -u -x`

These cover one/column/stream/long layouts, hidden entries, escape handling,
directory operands, indicators, unsorted enumeration, symbolic links, sizes,
inodes, numeric identities, recursion, reverse/size/time sorts, and timestamps.
BSD `-c` and `-u` select timestamps without implicitly enabling time sorting;
GNU uses them for time sorting when long format and an explicit sort are absent.
BSD `-f` keeps sorting disabled even if a later `-S` or `-t` appears.

## Conflicting short options

| Option | macOS / `--compat=bsd` | Linux / `--compat=gnu` |
| --- | --- | --- |
| `-B` | Octal escaping | Ignore backup names ending in `~` |
| `-D` | Custom strftime date (requires format) | Emacs dired offsets |
| `-G` | Automatic color | Omit group in long output |
| `-g` | Omit owner when long output is selected | Enable long format without owner |
| `-I` | Disable root's implicit `-A` | Ignore glob (requires pattern) |
| `-T` | Show seconds and year | Tab width (requires integer) |
| `-U` | Birth timestamp | Directory order / no sorting |
| `-v` | Raw names | Version ordering |
| `-w` | Raw names | Output width (requires integer) |
| `-X` | Do not cross filesystems recursively | Sort by extension |

GNU long options are available in both dialects, avoiding these conflicts.

## macOS extensions

| Option | Behavior |
| --- | --- |
| `-@` | Extended attribute names and sizes in long output |
| `-e` | ACL entries in long output, read through native ACL APIs |
| `-O` | BSD file flags in long output |
| `-%` | Mark dataless entries and skip dataless directory traversal |
| `-W` | Include whiteouts returned by the filesystem |
| `-y` | Sort time and alphabetical tie-breaks in the same direction |
| `-,` | Locale thousands separator in file sizes |
| `-P` | Describe argument symlinks themselves |

BSD identity-column spacing, long-format attribute markers, default 512-byte
blocks, human-size suffixes, `-D` precedence over `-T`, root visibility, and
recursive headings are handled separately from GNU behavior.
Although some macOS manual text calls `-g` a historical no-op, the modern
Unix2003 behavior of the installed `/bin/ls` omits the owner; qs follows the
observed behavior. `-H` follows directory operands while preserving file-link
metadata, and BSD parsing stops at the first operand. BSD mode ignores GNU-only
environment variables such as `TIME_STYLE`, `QUOTING_STYLE`, and `LS_BLOCK_SIZE`.

On Linux, BSD-only attributes have no direct kernel equivalent: file flags are
unavailable through BSD `stat`, dataless/whiteout behavior applies only when the
filesystem exposes it, and `-e` uses Linux POSIX ACL entries instead.

## GNU long options

The parser accepts these complete option names:

```text
--all --almost-all --author --block-size --classify --color --context
--dereference --dereference-command-line
--dereference-command-line-symlink-to-dir --directory --dired --escape
--file-type --format --full-time --group-directories-first --help --hide
--hide-control-chars --human-readable --hyperlink --ignore --ignore-backups
--indicator-style --inode --kibibytes --literal --no-group --numeric-uid-gid
--quote-name --quoting-style --recursive --reverse --show-control-chars
--si --size --sort --tabsize --time --time-style --version --width --zero
```

GNU `-N`, `-Q`, and `-Z` are also available. Option bundles, `--name=value`,
required arguments as separate words, `--`, operands containing raw Unix bytes,
and `POSIXLY_CORRECT` argument termination are supported. Optional arguments
(`--color`, `--hyperlink`, `--classify`) must use `=`, like GNU ls.

## Validation and limits

The macOS differential suite currently passes 236 fixture cases against the
host's `/bin/ls`. Ordinary output is compared byte-for-byte along with exit
status. Color cases compare text and terminal attributes, allowing equivalent
SGR reset/framing sequences. The latest local release-binary results are saved
in [macos-parity.json](macos-parity.json). Run it with:

```sh
cargo build --locked
python3 scripts/macos-parity.py --save target/macos-parity.json
```

Fixtures cover all documented macOS short options, fixed-width column packing
at seven terminal widths, stream wrapping, `LS_COLWIDTHS`, block sizes, human
rounding, device numbers, regular files, hard links, FIFOs, sockets, symlinks,
ACLs and inherited ACLs, numeric ACL identities, file flags, extended attributes,
resource forks, filesystem boundaries, `LSCOLORS` precedence and defaults,
locale-specific date ordering and month padding, decimal/thousands separators,
and C, UTF-8, ISO-8859-1, and Shift-JIS locale behavior. Invalid filename
encodings use native collation fallback and report a failure status like ls.

The Rust suite separately checks the new output modes, native bulk metadata,
configuration, and error handling. Installer tests cover reversible shell
configuration updates. Linux runtime tests are configured in CI; local Linux
validation has been limited to cross-compilation checks.

Known differences / unverified edges:

- Dataless iCloud objects and whiteouts are supported by the existing native
  metadata/entry flags, but their fixtures could not be manufactured on the
  local APFS filesystem. Firmlink targets and network-filesystem races need
  additional live fixtures. Missing bulk attributes still fall back to stat.
- Historical `COMMAND_MODE=legacy`/`COMPAT_MODE` emulation is not implemented;
  the target is the current macOS Unix2003 behavior.
- The terminal palette, automatic color default, `NO_COLOR`, table/tree/JSON
  modes, configurable defaults, and Unicode icons are intentional qist features.
- Remaining GNU-only edges include custom `LS_COLORS` escape framing,
  `ln=target`, capability-specific colors, full gnulib quoting/translations, and
  every unit-label spelling for `--block-size`. These do not displace macOS
  semantics.
- A finite fixture suite does not establish universal equivalence across every
  macOS version, filesystem, locale, and flag combination.

These boundaries are explicit so a parser accepting a flag is not confused with
exhaustive behavioral parity. Expand differential fixtures before removing an
item or publishing a 100% compatibility claim.

References:

- [GNU coreutils ls manual](https://www.gnu.org/software/coreutils/manual/html_node/ls-invocation.html)
- [Apple ls source and manual](https://github.com/apple-oss-distributions/file_cmds/tree/main/ls)
- Local platform `man ls`, `man getattrlistbulk`, `man getattrlist`, and
  `man fstatat` are the runtime-platform references.
