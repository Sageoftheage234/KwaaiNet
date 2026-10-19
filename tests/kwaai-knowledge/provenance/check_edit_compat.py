"""Thin entry point: python -m provenance.check_edit_compat ... (same arguments as `python -m provenance.cli check-edit`)."""
import sys

from .cli import main

if __name__ == "__main__":
    raise SystemExit(main(["check-edit", *sys.argv[1:]]))
