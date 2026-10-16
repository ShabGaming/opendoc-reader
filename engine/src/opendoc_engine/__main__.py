"""Entry point: `python -m opendoc_engine` serves JSON-RPC on stdin and stdout."""

from __future__ import annotations

import logging
import sys

from opendoc_engine.methods import build_server


def main() -> int:
    # Logs on stderr only; stdout is reserved for protocol messages.
    logging.basicConfig(
        stream=sys.stderr,
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )
    # The wire format is UTF-8 regardless of the console code page.
    sys.stdin.reconfigure(encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    build_server().serve(sys.stdin, sys.stdout)
    return 0


if __name__ == "__main__":
    sys.exit(main())
