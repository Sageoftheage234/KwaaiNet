"""Thin entry point: python -m provenance.check_binary ... (same arguments as `python -m provenance.cli check-binary`)."""
import sys

from .cli import main

if __name__ == "__main__":
    raise SystemExit(main(["check-binary", *sys.argv[1:]]))
