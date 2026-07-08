"""
Backward-compatible entry point for YOLO detection.

Delegates to the shared yolo_detect module used by run_yolos.py.

Run from _data/3d_data_v2/:
    uv run python street_cleanup/run_yolo_top_down.py
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from street_cleanup.run_yolos import main

if __name__ == "__main__":
    main()
