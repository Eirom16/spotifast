#!/usr/bin/env python3
"""Physical listening-history journey; shared profile/install/capture checks."""
from pathlib import Path
import runpy
import sys

if __name__ == '__main__':
    sys.argv.append('--history')
    runpy.run_path(str(Path(__file__).with_name('android-artwork-qa.py')), run_name='__main__')
