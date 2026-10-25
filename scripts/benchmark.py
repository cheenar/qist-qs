#!/usr/bin/env python3
"""Compare equivalent warm-cache listings; create fixtures only in a temporary directory."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import tempfile
import time

def clean_environment():
    ignored = {
        "LS_COLORS", "LSCOLORS", "CLICOLOR", "CLICOLOR_FORCE", "BLOCKSIZE",
        "BLOCK_SIZE", "LS_BLOCK_SIZE", "TIME_STYLE", "QUOTING_STYLE",
        "POSIXLY_CORRECT", "LS_SAMESORT", "LS_COLWIDTHS", "COMMAND_MODE",
        "COMPAT_MODE", "QIST_NO_BULK",
    }
    env = {key: value for key, value in os.environ.items() if key not in ignored}
    env.update(LC_ALL="C", NO_COLOR="1", TZ="UTC")
    return env


def identify(command):
    result = subprocess.run(command, capture_output=True, text=True, check=True)
    return result.stdout.strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--entries", type=int, default=20000)
    parser.add_argument("--runs", type=int, default=31)
    parser.add_argument("--warmups", type=int, default=3)
    parser.add_argument("--binary", type=Path, default=Path(__file__).resolve().parents[1] / "target/release/qs")
    parser.add_argument("--save", type=Path)
    args = parser.parse_args()
    if args.entries < 1 or args.runs < 1 or args.warmups < 0:
        parser.error("entries/runs must be positive and warmups must be nonnegative")
    binary = args.binary.expanduser().resolve(strict=True)
    cpu = platform.processor() or platform.machine()
    if platform.system() == "Darwin":
        cpu = identify(["sysctl", "-n", "machdep.cpu.brand_string"])
    env = clean_environment()
    report = {
        "schema_version": 2,
        "recorded_at": datetime.now(timezone.utc).isoformat(),
        "binary_version": identify([str(binary), "--version"]),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "ls_sha256": hashlib.sha256(Path("/bin/ls").read_bytes()).hexdigest(),
        "rustc": identify(["rustc", "--version"]),
        "platform": platform.platform(), "cpu": cpu,
        "entries": args.entries, "runs": args.runs, "warmups": args.warmups,
        "fixture": "empty regular files; shuffled creation order; seed 42",
        "cache": "warm", "output": "/dev/null",
        "environment": {"LC_ALL": "C", "NO_COLOR": "1", "TZ": "UTC"},
        "order": "qs first on even iterations; ls first on odd iterations",
        "timing": "wall clock, including process launch; excludes fixture creation",
        "results": [],
    }
    with tempfile.TemporaryDirectory(prefix="qist-bench-") as directory:
        root = Path(directory)
        indices = list(range(args.entries))
        random.Random(42).shuffle(indices)
        for i in indices:
            (root / f"entry-{i:08d}.txt").touch()
        for flags in [["-1"], ["-ln"], ["-1f"]]:
            commands = [[str(binary), "--no-config", *flags, directory], ["/bin/ls", *flags, directory]]
            outputs = [subprocess.run(command, env=env, capture_output=True, check=True).stdout for command in commands]
            if outputs[0] != outputs[1]:
                raise RuntimeError(f"outputs differ for {flags}; refusing to benchmark different work")
            samples = [[], []]
            for command in commands:
                for _ in range(args.warmups):
                    subprocess.run(command, env=env, stdout=subprocess.DEVNULL, check=True)
            for run in range(args.runs):
                for index in ([0, 1] if run % 2 == 0 else [1, 0]):
                    start = time.perf_counter_ns()
                    subprocess.run(commands[index], env=env, stdout=subprocess.DEVNULL, check=True)
                    samples[index].append((time.perf_counter_ns() - start) / 1_000_000)
            for label, times in zip(["qs", "ls"], samples):
                result = {
                    "command": label, "flags": flags, "output_matches": True,
                    "median_ms": round(statistics.median(times), 3),
                    "min_ms": round(min(times), 3), "max_ms": round(max(times), 3),
                    "samples_ms": [round(t, 6) for t in times],
                }
                report["results"].append(result)
                print(f"{label:5} {' '.join(flags):5} {result['median_ms']:9.3f} ms median", flush=True)
    if args.save:
        args.save.parent.mkdir(parents=True, exist_ok=True)
        args.save.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
