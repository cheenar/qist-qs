#!/usr/bin/env python3
"""Capture real qs output and render a silent, captioned GitHub demo with Pillow/FFmpeg."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT, FPS = 1600, 1000, 8
BG, PANEL, BORDER = '#0b0e14', '#121823', '#273244'
TEXT, MUTED, ACCENT, PURPLE = '#e8edf5', '#9aa9bd', '#8aefc2', '#b7b1ff'
ANSI = ['#627088', '#ff8d97', '#8aefc2', '#f4cf91', '#93b9ff', '#c8a8ff', '#8cdae5', TEXT]


def fonts(path):
    candidates = [path] if path else [
        '/System/Library/Fonts/SFNSMono.ttf',
        '/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf',
    ]
    for candidate in candidates:
        if candidate and Path(candidate).exists():
            return {size: ImageFont.truetype(candidate, size) for size in [20, 22, 24, 26, 28, 32, 40, 48, 60, 100, 150]}
    raise RuntimeError('No monospace font found. Supply --font=/path/to/font.ttf')


def run(command, **kwargs):
    return subprocess.run(command, check=True, **kwargs)


def fixture(root):
    files = {
        'Cargo.toml': '[package]\nname = "atlas"\nversion = "0.1.0"\n',
        'README.md': '# Atlas\n\nA small workspace for the qist demonstration.\n',
        'config.toml': 'default_output = "table"\nhuman_readable = true\ngroup_directories_first = true\n',
        '.gitignore': '/target/\n',
        'src/main.rs': 'fn main() { println!("hello, atlas"); }\n',
        'src/fs.rs': '// Native filesystem operations.\n',
        'src/render.rs': '// Terminal-aware output.\n',
        'docs/guide.md': '# Atlas guide\n',
        'docs/notes.md': '# Development notes\n',
        'tests/cli.rs': '// Command-line checks.\n',
        'run.sh': '#!/bin/sh\nprintf "hello, atlas\\n"\n',
    }
    for name, content in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    (root / 'src/fs.rs').write_bytes(b'// Native filesystem operations.\n' * 72)
    (root / 'src/render.rs').write_bytes(b'// Terminal-aware output.\n' * 120)
    (root / 'run.sh').chmod(0o755)
    (root / 'latest').symlink_to('src')
    for path in sorted(root.rglob('*'), reverse=True):
        os.utime(path, (1792929600, 1792929600), follow_symlinks=False)


def captures(binary):
    from benchmark import clean_environment
    env = clean_environment()
    env.pop('NO_COLOR', None)
    env.update(LC_ALL='en_US.UTF-8', TERM='xterm-256color', COLUMNS='96', TZ='UTC')
    definitions = [
        ('table', 'Metadata, without the squint.', 'Permissions, ownership, sizes, and dates in a labeled table.',
         ['--no-config', '--table', '-h', '--color=always'], 'qs --no-config --table -h --color=always', 9),
        ('tree', 'See the shape of a project.', 'Depth limits, symlinks, and directory counts.',
         ['--no-config', '--tree', '--depth=2', '--color=always'], 'qs --no-config --tree --depth=2 --color=always', 8),
        ('json', 'For people. And programs.', 'A versioned JSON envelope, with exact filename bytes.',
         ['--no-config', '--json', 'docs'], "qs --no-config --json docs | jq '{schema_version, files: [.roots[].entries[].name]}'", 7),
        ('config', 'Your default. Your terminal.', 'This fixture config sets table output and human-readable sizes.',
         ['--config=config.toml', '--color=always'], 'qs --config=config.toml --color=always', 7),
    ]
    result = []
    with tempfile.TemporaryDirectory(prefix='qist-demo-') as name:
        root = Path(name)
        fixture(root)
        for key, title, subtitle, flags, display, duration in definitions:
            completed = run([str(binary), *flags], cwd=root, env=env, capture_output=True, text=True)
            stdout = completed.stdout
            if key == 'json':
                raw_stdout = stdout
                stdout = run(['jq', '{schema_version, files: [.roots[].entries[].name]}'],
                             input=stdout, capture_output=True, text=True).stdout
            else:
                raw_stdout = stdout
            result.append(dict(key=key, title=title, subtitle=subtitle, args=flags,
                               command=display, stdout=stdout, raw_stdout=raw_stdout, stderr=completed.stderr,
                               status=completed.returncode, duration=duration))
    return result


def text(draw, xy, value, font, fill=TEXT):
    draw.text(xy, value, font=font, fill=fill)


def base(f, section, number):
    image = Image.new('RGB', (WIDTH, HEIGHT), BG)
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((70, 56, 134, 120), radius=17, fill=PANEL, outline=BORDER, width=2)
    text(draw, (79, 66), 'qs', f[32], ACCENT)
    text(draw, (156, 70), 'qist / quick list', f[28])
    text(draw, (1090, 76), 'RUST  /  macOS FIRST', f[20], MUTED)
    draw.line((72, 143, 1528, 143), fill=BORDER, width=2)
    text(draw, (74, 942), section, f[20], MUTED)
    text(draw, (1434, 942), f'{number:02d}/08', f[20], MUTED)
    return image, draw


def ansi_line(draw, position, line, f):
    x, y = position
    color, bold, dim = TEXT, False, False
    for token in re.split(r'(\x1b\[[0-9;]*m)', line.expandtabs(8)):
        if token.startswith('\x1b['):
            for part in token[2:-1].split(';'):
                code = int(part or 0)
                if code == 0: color, bold, dim = TEXT, False, False
                elif code == 1: bold = True
                elif code == 2: dim = True
                elif code == 22: bold, dim = False, False
                elif 30 <= code <= 37: color = ANSI[code - 30]
                elif 90 <= code <= 97: color = ANSI[code - 90]
                elif code == 39: color = TEXT
            continue
        fill = MUTED if dim else color
        text(draw, (x, y), token, f[24], fill)
        if bold:
            text(draw, (x + 0.5, y), token, f[24], fill)
        x += draw.textlength(token, font=f[24])
        if x > 1508:
            raise RuntimeError('Terminal output exceeds video width; adjust fixture or font')


def terminal(f, capture, elapsed, number):
    image, draw = base(f, 'ACTUAL CLI OUTPUT / PACED PLAYBACK', number)
    text(draw, (74, 182), capture['title'], f[48])
    text(draw, (76, 256), capture['subtitle'], f[24], MUTED)
    draw.rounded_rectangle((72, 324, 1528, 911), radius=20, fill=PANEL, outline=BORDER, width=2)
    for index, color in enumerate(['#ed7a80', '#f4cf91', '#8aefc2']):
        draw.ellipse((98 + index * 24, 347, 111 + index * 24, 360), fill=color)
    text(draw, (664, 340), 'atlas / zsh', f[20], MUTED)
    command = '$ ' + capture['command']
    count = min(len(command), int(max(0, elapsed - 0.2) * 85))
    text(draw, (100, 390), command[:count], f[24], ACCENT)
    lines = capture['stdout'].splitlines()
    shown = min(17, int(max(0, elapsed - 1.3) * 24))
    for row, line in enumerate(lines[:shown]):
        ansi_line(draw, (100, 440 + row * 26), line, f)
    if len(lines) > 17 and elapsed > 2:
        text(draw, (100, 874), 'Excerpt shown; complete output is saved in demo-capture.json.', f[20], MUTED)
    return image


def intro(f, elapsed):
    image, draw = base(f, 'A NATIVE DIRECTORY LISTER', 1)
    text(draw, (92, 240), 'qist', f[150])
    text(draw, (100, 445), 'Quick list.', f[60], ACCENT)
    text(draw, (103, 555), 'macOS instincts. More ways to see.', f[40])
    text(draw, (105, 642), 'classic ls   /   table   /   tree   /   JSON', f[28], MUTED)
    text(draw, (105, 811), '$ make install', f[28], PURPLE)
    return image


def pairs(report):
    return [(report['results'][i], report['results'][i + 1]) for i in range(0, 6, 2)]


def large_benchmark(f, report, elapsed):
    image, draw = base(f, 'MEASURED DATA / ANIMATED BARS', 6)
    text(draw, (74, 184), '20,000 files. Equivalent work.', f[48])
    text(draw, (76, 258), f"{report['cpu']} / warm cache / {report['runs']} runs / median wall time / /dev/null", f[24], MUTED)
    progress = min(1, max(0, (elapsed - 0.3) / 1.2))
    progress = 1 - (1 - progress) ** 3
    for i, (ours, native) in enumerate(pairs(report)):
        y = 370 + i * 155
        label = ['Sorted names  -1', 'Numeric details  -ln', 'Unsorted names  -1f'][i]
        text(draw, (78, y), label, f[26])
        ratio = native['median_ms'] / ours['median_ms']
        winner = f'qs {ratio:.2f}x' if ratio >= 1 else f'ls {1 / ratio:.2f}x'
        text(draw, (1250, y), winner, f[24], ACCENT if ratio >= 1 else PURPLE)
        maximum = max(ours['median_ms'], native['median_ms'])
        for j, (row, color) in enumerate([(ours, ACCENT), (native, PURPLE)]):
            by = y + 45 + j * 36
            text(draw, (80, by - 5), row['command'], f[22], color)
            length = 980 * row['median_ms'] / maximum * progress
            draw.rounded_rectangle((160, by, 160 + max(2, length), by + 23), radius=5, fill=color)
            text(draw, (1190, by - 6), f"{row['median_ms']:.3f} ms", f[22], color)
    text(draw, (78, 858), 'Each pair shares a scale. Output is verified equal before timing.', f[22], MUTED)
    return image


def small_benchmark(f, report, elapsed):
    image, draw = base(f, 'THE SMALL-DIRECTORY TRADEOFF', 7)
    text(draw, (74, 184), 'Small directories. Launch costs.', f[48])
    text(draw, (76, 258), '100 files / same protocol / process launch is included', f[24], MUTED)
    text(draw, (104, 382), 'listing', f[26], MUTED)
    text(draw, (780, 382), 'qs', f[26], ACCENT)
    text(draw, (1150, 382), '/bin/ls', f[26], PURPLE)
    for i, (ours, native) in enumerate(pairs(report)):
        y = 468 + i * 90
        label = ['Sorted names  -1', 'Numeric details  -ln', 'Unsorted names  -1f'][i]
        text(draw, (104, y), label, f[28])
        text(draw, (780, y), f"{ours['median_ms']:.3f} ms", f[28], ACCENT)
        text(draw, (1150, y), f"{native['median_ms']:.3f} ms", f[28], PURPLE)
    text(draw, (104, 807), 'Local measurements, not a universal speed claim.', f[28])
    text(draw, (104, 858), 'Cold caches, terminals, locales, and other disks can change results.', f[22], MUTED)
    return image


def outro(f, elapsed, version):
    image, draw = base(f, 'INSTALL / EXPLORE / REPRODUCE', 8)
    text(draw, (80, 218), 'Your next listing starts here.', f[48])
    for i, line in enumerate(['$ make install', '$ qs --table -h', '$ qs --tree --depth=2', '$ qs --json src']):
        text(draw, (104, 355 + i * 88), line, f[40], ACCENT if i == 0 else TEXT)
    text(draw, (104, 791), 'README.md / GUIDE.md / docs/BENCHMARKS.md', f[28], MUTED)
    text(draw, (104, 854), f'{version} / macOS + Linux / MIT', f[24], MUTED)
    return image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/qs')
    parser.add_argument('--large-report', type=Path, default=ROOT / 'docs/benchmark-macos.json')
    parser.add_argument('--small-report', type=Path, default=ROOT / 'docs/benchmark-macos-small.json')
    parser.add_argument('--output', type=Path, default=ROOT / 'docs/assets')
    parser.add_argument('--font')
    args = parser.parse_args()
    if not shutil.which('ffmpeg') or not shutil.which('jq'):
        parser.error('FFmpeg (including libx264) and jq are required')
    binary = args.binary.resolve(strict=True)
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    reports = [json.loads(p.read_text()) for p in [args.large_report, args.small_report]]
    for report in reports:
        if report.get('binary_sha256') != digest:
            parser.error('Benchmark binary differs from demo binary. Regenerate both reports first.')
        if any(not row.get('output_matches') for row in report['results']):
            parser.error('Benchmark reports must confirm equivalent output')
        if len(report['results']) != 6:
            parser.error('This storyboard requires all three benchmark pairs')
    if [r['entries'] for r in reports] != [20000, 100]:
        parser.error('This storyboard requires 20,000- and 100-file reports')
    args.output.mkdir(parents=True, exist_ok=True)
    f = fonts(args.font)
    captured = captures(binary)
    provenance = {
        'generated_at': datetime.now(timezone.utc).isoformat(),
        'binary_version': reports[0]['binary_version'], 'binary_sha256': digest,
        'presentation': 'Real CLI stdout rendered in a terminal layout; typing/reveal/bar timing is editorial, not execution time.',
        'benchmark_sources': [p.name for p in [args.large_report, args.small_report]],
        'benchmark_sha256': [hashlib.sha256(p.read_bytes()).hexdigest() for p in [args.large_report, args.small_report]],
        'captures': captured,
    }
    (args.output / 'demo-capture.json').write_text(json.dumps(provenance, indent=2) + '\n')
    scenes = [(4, lambda t: intro(f, t))]
    scenes += [(c['duration'], lambda t, c=c, i=i: terminal(f, c, t, i + 2)) for i, c in enumerate(captured)]
    scenes += [(10, lambda t: large_benchmark(f, reports[0], t)),
               (6, lambda t: small_benchmark(f, reports[1], t)),
               (4, lambda t: outro(f, t, reports[0]['binary_version'].split(' (')[0]))]
    with tempfile.TemporaryDirectory(prefix='qist-frames-') as name:
        frames = Path(name)
        index = 0
        for scene_number, (duration, render) in enumerate(scenes):
            print(f'Rendering scene {scene_number + 1}/{len(scenes)}', flush=True)
            for frame in range(duration * FPS):
                image = render(frame / FPS)
                image.save(frames / f'{index:05d}.png')
                if scene_number == 1 and frame == 4 * FPS:
                    image.save(args.output / 'qist-demo-poster.png')
                index += 1
        video = args.output / 'qist-demo.mp4'
        run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-framerate', str(FPS),
             '-i', str(frames / '%05d.png'), '-r', '24', '-c:v', 'libx264', '-preset', 'medium',
             '-crf', '20', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', str(video)])
        preview = args.output / 'qist-demo.gif'
        run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-i', str(video),
             '-filter_complex', '[0:v]fps=6,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=96:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=3',
             '-loop', '0', str(preview)])
    print(f'Created {video} ({video.stat().st_size / 1024**2:.2f} MiB)', flush=True)
    print(f'Created {preview} ({preview.stat().st_size / 1024**2:.2f} MiB)', flush=True)


if __name__ == '__main__':
    main()
