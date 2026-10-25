# qist demo and GitHub presentation

The repository includes a **55-second**, silent, captioned demonstration:

- [H.264 MP4, 1600 × 1000](assets/qist-demo.mp4)
- [Inline animated GIF, 960 × 600](assets/qist-demo.gif)
- [Poster image](assets/qist-demo-poster.png)
- [CLI captures and provenance](assets/demo-capture.json)

The README embeds the GIF and links it to the full MP4. This gives a working,
animated demonstration with repository-relative links before a hosted video
attachment exists. It requires no external media host.

## Storyboard

| Time | Demonstration |
| --- | --- |
| 0–4 s | qist introduction and installation command |
| 4–13 s | Actual `--table -h` output, with terminal colors |
| 13–21 s | Actual `--tree --depth=2` output |
| 21–28 s | Actual `--json` output projected with `jq` |
| 28–35 s | A real fixture config selecting table output by default |
| 35–45 s | Measured 20,000-file comparisons for all three workloads |
| 45–51 s | The 100-file cases where `/bin/ls` was faster |
| 51–55 s | Install and usage commands |

The fixture is a disposable sample project, not a scan of a personal home
folder. It contains directories, source files, an executable, a hidden file,
and a symlink. The captured stdout and exit status are real. JSON's displayed
projection comes from `jq`; the unprojected document is also saved as
`raw_stdout` in the capture report. Long output can be explicitly labeled as an
excerpt when it exceeds the display.

Typing, line reveals, and chart growth are editorial animations. **Video time
is not execution time.** Benchmark values come from the saved reports, not the
animation. The film is silent by design; every claim is readable on screen.

## Regenerate

Media tools are development-only dependencies: Python 3 with **Pillow**,
**FFmpeg** with `libx264`, **jq**, and a monospace TrueType font. They are not
needed to build or run `qs`.

For example, create a local Python environment:

```sh
python3 -m venv target/demo-venv
target/demo-venv/bin/python -m pip install Pillow
```

Install FFmpeg and jq through your system's package manager. Then:

```sh
make build
# Fresh reports must describe this exact binary:
python3 scripts/benchmark.py --entries 20000 --runs 31 --save docs/benchmark-macos.json
python3 scripts/benchmark.py --entries 100 --runs 31 --save docs/benchmark-macos-small.json
make demo DEMO_PYTHON=target/demo-venv/bin/python
```

`make demo` reuses the two saved reports; it does not rerun benchmarks. The
renderer rejects reports whose binary hashes differ or whose workloads do not
match the storyboard. CPU and run-count labels come from the reports. Changing
fixture sizes, workloads, or version labels requires updating the storyboard.
Also update the README and benchmark tables when publishing new measurements.

Default font discovery supports macOS SF Mono and Linux DejaVu Sans Mono. To
select another font or output directory:

```sh
target/demo-venv/bin/python scripts/demo.py --font=/path/to/Mono.ttf --output=target/demo
```

All sample-project and rendering frames are created in temporary directories.
Only the final MP4, GIF, poster, and provenance JSON are retained. No screen
recording permission or access to personal terminal windows is required.

## GitHub publishing

Use the contents of this project folder as the repository root. Its README,
license, Cargo files, CI workflow, guide, benchmarks, and media use relative
links. There is no hard-coded owner/repository URL or placeholder badge.

The committed GIF renders inline in README Markdown. The MP4 link opens the
repository media file; a repository-relative MP4 link is not assumed to become
an inline video player.

For GitHub's native video player, upload `assets/qist-demo.mp4` from this docs
folder through GitHub's attachment UI and insert the returned attachment URL
into the README's demo section. A draft issue can be used to obtain that URL;
you do not need to publish the issue. Keep the GIF as the preview/fallback.
GitHub documents [video attachments and upload limits](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/attaching-files),
including H.264 as its recommended compatible codec. Both generated assets are
kept below the 10 MB free-plan attachment limit.

Before publishing:

```sh
make check
make test
make parity
```

Open the GIF and MP4 to verify text and playback. Check the benchmark tables
against their reports. The latest saved compatibility run is evidence for its
fixture suite, not a claim of exhaustive equivalence. This preparation does not
create a repository, upload media, or push anything to GitHub.
