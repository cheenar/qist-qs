#!/usr/bin/env python3
"""Compare qs with /bin/ls using disposable macOS filesystem fixtures."""
import argparse
import difflib
import json
import os
from pathlib import Path
import pwd
import re
import socket
import subprocess
import sys
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=Path, default=Path(__file__).resolve().parents[1] / "target/debug/qs")
parser.add_argument("--save", type=Path)
parser.add_argument("--filter", default="")
args = parser.parse_args()
if args.save:
    args.save.unlink(missing_ok=True)
if sys.platform != "darwin":
    parser.exit(1, "This differential suite requires macOS.\n")
binary = args.binary.resolve()
env = {key: value for key, value in os.environ.items() if key not in {
    "LS_COLORS", "LSCOLORS", "CLICOLOR", "CLICOLOR_FORCE", "COLORTERM", "NO_COLOR",
    "BLOCKSIZE", "BLOCK_SIZE", "LS_BLOCK_SIZE", "TIME_STYLE", "QUOTING_STYLE",
    "POSIXLY_CORRECT", "LS_SAMESORT", "LS_COLWIDTHS", "COLUMNS", "TABSIZE", "QIST_NO_BULK",
}}
env.update(LC_ALL="C", TZ="UTC", TERM="xterm-256color", COLUMNS="80")
results = []


def styled_text(raw):
    # Compare terminal appearance, allowing equivalent SGR/reset encodings.
    text = raw.decode("utf-8", "backslashreplace")
    state = [None, None, False]
    rendered = []
    for token in re.split(r"(\x1b\[[0-9;]*m|\x1b\([A-Z])", text):
        if token.startswith("\x1b["):
            for word in token[2:-1].split(";"):
                code = int(word or 0)
                if code == 0: state = [None, None, False]
                elif code == 1: state[2] = True
                elif code == 22: state[2] = False
                elif 30 <= code <= 37: state[0] = code
                elif 40 <= code <= 47: state[1] = code
                elif code == 39: state[0] = None
                elif code == 49: state[1] = None
        elif not token.startswith("\x1b("):
            rendered.extend((char, tuple(state)) for char in token)
    return rendered


def compare(label, flags, operands, overrides=None, color=False):
    if args.filter and args.filter not in label:
        return
    case_env = dict(env, **(overrides or {}))
    ours = subprocess.run([str(binary), "--no-config", *flags, *map(str, operands)], env=case_env, capture_output=True)
    native = subprocess.run(["/bin/ls", *flags, *map(str, operands)], env=case_env, capture_output=True)
    equal = styled_text(ours.stdout) == styled_text(native.stdout) if color else ours.stdout == native.stdout
    passed = ours.returncode == native.returncode and equal
    result = {"case": label, "passed": passed, "qs_status": ours.returncode, "ls_status": native.returncode}
    if not passed:
        diff = "".join(difflib.unified_diff(native.stdout.decode("utf-8", "backslashreplace").splitlines(keepends=True), ours.stdout.decode("utf-8", "backslashreplace").splitlines(keepends=True), fromfile="ls", tofile="qs"))
        result.update(diff=diff, qs_stderr=ours.stderr.decode("utf-8", "backslashreplace"), ls_stderr=native.stderr.decode("utf-8", "backslashreplace"))
        print(f"FAIL {label}\n{diff[:2200]}")
    results.append(result)


with tempfile.TemporaryDirectory(prefix="qs-parity-") as directory:
    root = Path(directory)
    files = root / "names"
    files.mkdir()
    names = ["a", "alphabet", "b.txt", "file10", "file2", "really-long-name.txt", "a b", "quote\"file", "back\\slash", "line\nfeed", "tab\tfile", "文件", "café", "combining-e\u0301", ".hidden"]
    for i, name in enumerate(names):
        (files / name).write_bytes(b"x" * (i + 1))
    (files / "dir").mkdir()
    (files / "dir/nested").write_bytes(b"nested")
    (files / "exec").write_bytes(b"#!/bin/sh\n")
    (files / "exec").chmod(0o755)
    (files / "link").symlink_to("a")
    (files / "dirlink").symlink_to("dir")
    (files / "broken").symlink_to("missing")
    os.link(files / "a", files / "hardlink")
    os.mkfifo(files / "fifo")
    sock = socket.socket(socket.AF_UNIX)
    sock.bind(str(files / "socket"))
    for flags in ["-1", "-a1", "-A1", "-I1", "-AI1", "-IA1", "-1r", "-1S", "-1t", "-1ty", "-1try", "-1fS", "-1F", "-1p", "-1L", "-1s", "-1is", "-1b", "-1B", "-1q", "-ln", "-lnO", "-lno", "-lng", "-lngo", "-lnF", "-lnp", "-lnh", "-lns", "-lnis", "-ln@", "-ln@h", "-ln@k", "-lnT", "-lnU", "-lnu", "-lnc", "-lnhk", "-lnkh", "-1R", "-1aR", "-1LR", "-1W", "-ln%"]:
        compare("basic " + flags, [flags], [files])
    for width in [8, 16, 20, 40, 80, 160, 200]:
        for flags in ["-C", "-x", "-m", "-Ci", "-Cis", "-CF", "-Cb", "-Cq", "-xF"]:
            compare(f"layout {flags} width={width}", [flags], [files], {"COLUMNS": str(width)})
    for widths in ["10:5:4:12:12:12:10:30", "0:0:0:0:0:0:0:30", "1:1:1:1:1:1:1:1", "invalid"]:
        for flags in ["-lnisO", "-Cis", "-1is"]:
            compare(f"column-minima {flags} {widths}", [flags], [files], {"LS_COLWIDTHS": widths})
    for block in ["1", "513", "768", "1K", "10K", "1m", "0", "invalid"]:
        compare("blocksize " + block, ["-lns"], [files], {"BLOCKSIZE": block})
    compare("ignore GNU environment", ["-lns"], [files], {"LS_BLOCK_SIZE": "1M", "BLOCK_SIZE": "1M", "TIME_STYLE": "bad", "QUOTING_STYLE": "bad"})
    for flags in ["-1H", "-lnH", "-lnL", "-lnP", "-lnHL", "-lnLH", "-lnLP", "-lnPH", "-dlnH"]:
        compare("link operand " + flags, [flags], [files / "link", files / "dirlink"])
    compare("options after operand", [str(files / "a"), "-l"], [])
    compare("unknown option", ["--not-real"], [])
    compare("missing operand", ["-1"], [root / "missing"])
    for device in ["/dev/null", "/dev/zero", "/dev/random"]:
        for flags in ["-ln", "-lnh", "-lnsO", "-lnF"]:
            compare(f"device {device} {flags}", [flags], [device])
    sized = root / "sizes"
    sized.mkdir()
    for size in [0, 1, 999, 1000, 1023, 1024, 1025, 9999, 10239, 10240, 99999, 1023999, 1024000, 1048576, 1073741824, 1099511627776]:
        with (sized / str(size)).open("wb") as stream:
            stream.truncate(size)
    compare("human sizes", ["-lnh"], [sized])
    dates = root / "dates"
    dates.mkdir()
    now = int(time.time())
    for name, stamp in [("recent", now), ("future", now + 7200), ("near-future", now + 86400 * 60), ("old", now - 86400 * 200), ("far-future", now + 86400 * 200)]:
        (dates / name).touch()
        os.utime(dates / name, (stamp, stamp))
    for locale in ["C", "en_US.UTF-8", "en_GB.UTF-8", "de_DE.UTF-8", "fr_FR.UTF-8", "hi_IN.UTF-8", "fr_FR.ISO8859-1", "de_DE.ISO8859-1", "ja_JP.SJIS"]:
        compare("date locale " + locale, ["-ln"], [dates], {"LC_ALL": locale})
        compare("date full " + locale, ["-lnT"], [dates], {"LC_ALL": locale})
        compare("grouped size " + locale, ["-ln,"], [sized], {"LC_ALL": locale})
        compare("human size locale " + locale, ["-lnh"], [sized], {"LC_ALL": locale})
        for flag in ["-1q", "-1b", "-1B"]:
            compare(f"filename locale {locale} {flag}", [flag], [files], {"LC_ALL": locale})
    for flags in [["-ln", "-D", "%Y", "-T"], ["-ln", "-D", ""], ["-ln", "-D", "%Y\n%H"], ["-ln", "-D", "%s"]]:
        compare("custom date " + repr(flags), flags, [dates])
    acl_file = root / "acl-file"
    acl_file.touch()
    subprocess.run(["/bin/chmod", "+a", "everyone allow read,write,execute,readattr,writeattr,readextattr,writeextattr,readsecurity,writesecurity,chown", str(acl_file)], check=True)
    acl_dir = root / "acl-dir"
    acl_dir.mkdir()
    subprocess.run(["/bin/chmod", "+a", "everyone allow list,add_file,search,delete_child,add_subdirectory,file_inherit,directory_inherit,limit_inherit,only_inherit", str(acl_dir)], check=True)
    for flags in ["-dlne", "-dlne@", "-dlneO"]:
        compare("acl " + flags, [flags], [acl_file, acl_dir])
    user = pwd.getpwuid(os.getuid()).pw_name
    subprocess.run(["/bin/chmod", "+a", f"user:{user} allow read,readsecurity", str(acl_file)], check=True)
    subprocess.run(["/bin/chmod", "+a", "everyone deny write", str(acl_file)], check=True)
    (acl_dir / "inherited").touch()
    for flags in ["-dle", "-dlne"]:
        compare("acl identity and inheritance " + flags, [flags], [acl_file, acl_dir / "inherited"])
    cross = root / "cross"
    cross.mkdir()
    (cross / "device-filesystem").symlink_to("/dev")
    compare("filesystem boundary", ["-1RLX"], [cross])
    resource = root / "resource-fork"
    resource.write_bytes(b"data")
    subprocess.run(["/usr/bin/xattr", "-wx", "com.apple.ResourceFork", (b"fork" * 1024).hex(), str(resource)], check=True)
    subprocess.run(["/usr/bin/xattr", "-w", 'com.qist.space "quote"', "attribute value", str(resource)], check=True)
    for flags in ["-ln@", "-lnh", "-lns", "-ln@b", "-ln@B", "-ln@q"]:
        compare("resource fork " + flags, [flags], [resource])
    colors = root / "colors"
    colors.mkdir()
    for name, mode in [("regular", 0o644), ("executable", 0o755), ("setuid", 0o4755), ("setgid", 0o2755), ("nonexec-setuid", 0o4644)]:
        (colors / name).touch()
        (colors / name).chmod(mode)
    for name, mode in [("directory", 0o755), ("sticky", 0o1777), ("writable", 0o777)]:
        (colors / name).mkdir()
        (colors / name).chmod(mode)
    (colors / "symlink").symlink_to("regular")
    (colors / "broken").symlink_to("absent")
    os.mkfifo(colors / "fifo")
    for palette in ["exfxcxdxbxegedabagacadah", "ExGxFxdxCxegedabagacedah", "Ex"]:
        compare("terminal colors " + palette, ["-1", "--color=always"], [colors], {"LSCOLORS": palette, "LS_COLORS": "di=31:ln=32"}, color=True)
    flagged = root / "flagged"
    flagged.touch()
    subprocess.run(["/usr/bin/chflags", "nodump,hidden", str(flagged)], check=True)
    compare("file flags", ["-lnO"], [flagged])
    sock.close()

report = {"total": len(results), "passed": sum(r["passed"] for r in results), "failed": sum(not r["passed"] for r in results), "cases": results}
if args.save:
    args.save.parent.mkdir(parents=True, exist_ok=True)
    args.save.write_text(json.dumps(report, indent=2, ensure_ascii=True) + "\n")
print(f"{report['passed']}/{report['total']} cases passed; {report['failed']} failed")
sys.exit(1 if report["failed"] else 0)
