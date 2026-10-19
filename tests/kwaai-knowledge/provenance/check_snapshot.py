"""Thin entry point: python -m provenance.check_snapshot ... (same arguments as `python -m provenance.cli snapshot`)."""
import sys

from .cli import main

if __name__ == "__main__":
    raise SystemExit(main(["snapshot", *sys.argv[1:]]))
