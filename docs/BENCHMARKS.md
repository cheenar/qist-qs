# Benchmarking qist against macOS ls

These are local measurements of **equivalent classic listings**, not a universal
speed claim. Table, tree, and JSON are richer formats and are not timed against
`ls` here. Raw reports include every sample, binary hashes, toolchain, CPU,
platform, timestamp, and test parameters.

## Recorded results

Recorded on 2026-10-25 with **qs 0.2.0**, an **Apple M3**, macOS **27.0.1**, and
local temporary files. Each command gets three untimed warmups and 31 timed
runs. The table reports medians, including process-launch overhead.

### 20,000 empty regular files

| Listing | Flags | `qs` | `/bin/ls` | `ls` time ÷ `qs` time |
| --- | --- | ---: | ---: | ---: |
| Sorted names | `-1` | 19.118 ms | 35.460 ms | 1.85× |
| Numeric details | `-ln` | 127.315 ms | 146.491 ms | 1.15× |
| Unsorted names | `-1f` | 13.090 ms | 25.005 ms | 1.91× |

A ratio above 1 means `qs` was faster. The 1.85× ratio describes relative time;
it is not an 85% reduction in elapsed time. For sorted names, the measured
reduction is about 46%.

[Full report and all 31 samples per command](benchmark-macos.json).

### 100 empty regular files

| Listing | Flags | `qs` | `/bin/ls` | Faster in this run |
| --- | --- | ---: | ---: | --- |
| Sorted names | `-1` | 3.419 ms | 2.600 ms | `ls` |
| Numeric details | `-ln` | 3.768 ms | 2.885 ms | `ls` |
| Unsorted names | `-1f` | 2.664 ms | 2.003 ms | `ls` |

The small-directory advantage is with `ls`, by 0.66–0.88 ms in these medians.
Process launch and fixed costs are a larger part of the result here.

[Small-directory report and raw samples](benchmark-macos-small.json).

## Method

The [Python harness](../scripts/benchmark.py):

1. Creates disposable empty files in the OS temporary directory. Creation order
   is shuffled using seed 42 so alphabetical sorting does real work.
2. Uses a release `qs` binary with `--no-config` and the absolute `/bin/ls`.
   Relevant user color, formatting, block-size, compatibility, and diagnostic
   overrides are removed. Both commands use `LC_ALL=C`, `TZ=UTC`, and `NO_COLOR=1`.
3. Compares stdout byte-for-byte for each flag set before timing. A mismatch
   aborts the benchmark instead of comparing different work.
4. Warms both commands, then alternates execution order to reduce systematic
   first-run advantage. Neither benchmark runs concurrently with the other.
5. Measures wall time using `perf_counter_ns` with stdout directed to
   `/dev/null`. Timing includes process launch and listing/output generation,
   but excludes fixture creation and terminal drawing.
6. Saves the median, min, max, and every timed sample. Temporary files are
   removed when the harness exits normally.

`-ln` avoids user/group name lookup. `-1f` intentionally follows BSD semantics,
including `.` and `..`; both binaries do the same work. Normal listing output
and return status are checked by the separate compatibility suite.

## Reproduce

From the repository root:

```sh
make build
python3 scripts/benchmark.py --entries 20000 --runs 31 --save target/benchmark-large.json
python3 scripts/benchmark.py --entries 100 --runs 31 --save target/benchmark-small.json
```

Or use the Make shortcut:

```sh
make benchmark
make benchmark BENCHMARK_ARGS='--entries 100 --runs 31'
```

The shortcut writes `target/benchmark.json`; choose separate `--save` paths when
keeping multiple reports. Use `--binary=/path/to/qs` to measure another release
build, or `--warmups=N` to change warmup count. Do not compare a debug Cargo
build against the system utility.

To update the published results, save fresh reports at
`docs/benchmark-macos.json` and `docs/benchmark-macos-small.json`, then update both
Markdown tables and regenerate the [demo](DEMO.md). The demo checks binary hashes
to reject stale reports for a different build. Historical results are not
silently relabeled as a new version.

## What this does not establish

The benchmark is warm-cache, on one machine, with empty files in one flat
directory. It does not measure cold cache, recursive mixed trees, network
volumes, cloud hydration, interactive terminal rendering, energy, peak memory,
or every locale. It does not isolate syscalls from startup costs. Background
activity, OS changes, storage layout, and thermal state can affect results;
raw samples expose variation but do not eliminate it.

The native metadata fast path has fallbacks for directories, links, incomplete
attributes, and unsupported filesystems. Its advantage depends on the actual
workload. In particular, a 20,000-file result should not be advertised as the
performance of a typical small source directory. Run the harness on your own
machine before drawing broader conclusions.
