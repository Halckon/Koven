#!/usr/bin/env python3
"""Bounded, read-only Linux CI telemetry; never delete files or hide gate failures."""
import argparse
import os
from pathlib import Path
import subprocess
import signal
import time


def measure(command):
    """Let df/du stream only aggregate rows; a missing measurement is not a gate."""
    try:
        result = subprocess.run(command, timeout=15, check=False, stderr=subprocess.DEVNULL)
        if result.returncode:
            print(f'disk telemetry: {command[0]} exited {result.returncode}', flush=True)
    except (OSError, subprocess.TimeoutExpired) as error:
        print(f'disk telemetry unavailable: {error}', flush=True)


def sample(label, *, sizes):
    workspace = Path(os.environ.get('GITHUB_WORKSPACE', Path.cwd()))
    temporary = Path(os.environ.get('RUNNER_TEMP', '/tmp'))
    cargo = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo'))
    # Report overlapping aggregates separately; never add these rows together.
    paths = [workspace, workspace / 'target', temporary, cargo, Path('/tmp'),
             Path('/home/runner/actions-runner/cached')]
    existing = list(dict.fromkeys(str(path) for path in paths if path.exists()))
    print(f'disk telemetry: {label}; unix_seconds={int(time.time())}', flush=True)
    measure(['df', '-h', *existing])
    measure(['df', '-i', *existing])
    if sizes:
        # No recursive listing, root scan, sudo, cleanup, or new output file.
        for path in existing:
            measure(['du', '-sx', path])


def stop_monitor(signum, frame):
    # Raising inside subprocess.run kills and waits for its current child.
    raise SystemExit(0)


def monitor():
    # At most 360 samples, 60 seconds apart plus bounded measurement time.
    for index in range(360):
        sample(f'composition-{index}', sizes=index % 5 == 0)
        time.sleep(60)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('label')
    args = parser.parse_args()
    if args.label == 'monitor':
        signal.signal(signal.SIGTERM, stop_monitor)
        signal.signal(signal.SIGINT, stop_monitor)
        monitor()
    else:
        sample(args.label, sizes=True)


if __name__ == '__main__':
    main()
