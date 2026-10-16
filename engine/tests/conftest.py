"""Test setup shared by every test module."""

import os

# The demo engine is hidden from the real registry; the RPC tests use it as a stand-in.
os.environ.setdefault("OPENDOC_DEMO_ENGINE", "1")
