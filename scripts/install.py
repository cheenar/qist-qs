#!/usr/bin/env python3
"""Install qs and manage one idempotent PATH block in the selected zshrc."""
import argparse
from datetime import datetime, timezone
import os
from pathlib import Path
import shlex
import shutil
import stat
import tempfile

START = b"# >>> qs PATH >>>"
END = b"# <<< qs PATH <<<"


def path_block(bindir, home):
    try:
        relative = bindir.relative_to(home)
        expression = '"$HOME/' + str(relative).replace('\\', '\\\\').replace('"', '\\"').replace('$', '\\$').replace('`', '\\`') + '"'
    except ValueError:
        expression = shlex.quote(str(bindir))
    # Keeping the variable local to this block avoids global zsh option changes.
    return b"\n".join([
        START,
        ("_qs_path_dir=" + expression).encode(),
        ("case \":${PATH-}:\" in").encode(),
        b'  *":${_qs_path_dir}:"*) ;;',
        b'  *) export PATH="${_qs_path_dir}${PATH:+:$PATH}" ;;',
        b"esac",
        b"unset _qs_path_dir",
        END,
    ]) + b"\n"


def updated_rc(original, block):
    starts = original.count(START)
    ends = original.count(END)
    if starts == ends == 0:
        return original + (b"\n" if original and not original.endswith(b"\n") else b"") + block
    if starts != 1 or ends != 1:
        raise ValueError("zshrc contains duplicate or incomplete qs PATH markers; repair them before installing")
    start = original.index(START)
    end = original.index(END)
    if end < start or start > 0 and original[start - 1:start] != b"\n":
        raise ValueError("zshrc contains malformed qs PATH markers")
    end += len(END)
    if original[end:end + 2] == b"\r\n":
        end += 2
    elif original[end:end + 1] == b"\n":
        end += 1
    return original[:start] + block + original[end:]


def atomic_write(destination, data, mode):
    destination.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix=".qs-install-", dir=destination.parent)
    temporary = Path(name)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, mode)
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def install(binary, prefix, zshrc, home):
    binary = binary.expanduser().resolve(strict=True)
    prefix = prefix.expanduser().absolute()
    zshrc = zshrc.expanduser().absolute()
    if any("\n" in str(p) or "\r" in str(p) for p in [prefix, zshrc, home]):
        raise ValueError("installation paths cannot contain newlines")
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError(f"not an executable file: {binary}")
    # Preserve a symlinked .zshrc and update its actual target atomically.
    rc_target = zshrc.resolve()
    original = rc_target.read_bytes() if rc_target.exists() else b""
    block = path_block(prefix / "bin", home)
    updated = updated_rc(original, block)
    destination = prefix / "bin/qs"
    if destination.resolve() == rc_target or destination.resolve() == binary:
        raise ValueError("installation destination overlaps the source binary or zshrc")
    backup = None
    if updated != original:
        mode = stat.S_IMODE(rc_target.stat().st_mode) if rc_target.exists() else 0o644
        if rc_target.exists():
            stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
            backup = zshrc.with_name(zshrc.name + ".qs-backup-" + stamp)
            shutil.copy2(rc_target, backup)
        # Write the binary before making the PATH entry visible.
        atomic_write(destination, binary.read_bytes(), 0o755)
        atomic_write(rc_target, updated, mode)
    else:
        atomic_write(destination, binary.read_bytes(), 0o755)
    return destination, backup


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--prefix", type=Path, default=Path.home() / ".softwares")
    parser.add_argument("--zshrc", type=Path, default=Path.home() / ".zshrc")
    args = parser.parse_args()
    try:
        destination, backup = install(args.binary, args.prefix, args.zshrc, Path.home())
    except (OSError, ValueError) as error:
        parser.exit(1, f"qs install: {error}\n")
    print(f"Installed {destination}")
    if backup:
        print(f"Saved shell configuration backup: {backup}")
    print(f"PATH configured in {args.zshrc}. Open a new terminal or run: source {shlex.quote(str(args.zshrc))}")


if __name__ == "__main__":
    main()
