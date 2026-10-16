"""`python -m opendoc_engine` as Rust runs it: JSON lines in and out over real pipes."""

import json
import subprocess
import sys
import time

import pytest
from PIL import Image


@pytest.fixture
def engine():
    process = subprocess.Popen(
        [sys.executable, "-m", "opendoc_engine"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    yield process
    if process.poll() is None:
        process.kill()
    process.wait(10)
    for stream in (process.stdin, process.stdout, process.stderr):
        stream.close()


def send(process, message: dict) -> None:
    process.stdin.write((json.dumps(message) + "\n").encode("utf-8"))
    process.stdin.flush()


def receive(process) -> dict:
    return json.loads(process.stdout.readline().decode("utf-8"))


def ask(process, request_id: int, method: str, params: dict | None = None) -> dict:
    message = {"jsonrpc": "2.0", "id": request_id, "method": method}
    if params is not None:
        message["params"] = params
    send(process, message)
    while True:
        reply = receive(process)
        if reply.get("id") == request_id:
            return reply


def test_startup_handshake_and_a_demo_page(engine, tmp_path):
    started = time.monotonic()
    assert ask(engine, 1, "engine.ping")["result"] == "pong"
    assert time.monotonic() - started < 10  # heavy libraries are not imported yet
    version = ask(engine, 2, "engine.version")["result"]
    assert (version["irSchema"], version["protocol"]) == ("1", "1")
    ids = [e["id"] for e in ask(engine, 3, "engine.capabilities")["result"]]
    assert {"paddleocr-structure", "paddleocr-vl", "deepseek-ocr"} <= set(ids)

    configure = ask(
        engine,
        4,
        "engine.configure",
        {"llamaServerUrl": None, "workDir": str(tmp_path / "w"), "modelsDir": str(tmp_path / "m")},
    )
    assert "result" in configure

    image = tmp_path / "page.png"
    Image.new("RGB", (80, 100), "white").save(image)
    converted = ask(
        engine,
        5,
        "convert.page",
        {
            "engineId": "demo",
            "modelId": "none",
            "imagePath": str(image),
            "pageIndex": 0,
            "irPath": str(tmp_path / "ir.json"),
            "cropsDir": str(tmp_path / "crops"),
        },
    )
    assert converted["result"]["blockCount"] == 4
    assert json.loads((tmp_path / "ir.json").read_text("utf-8"))["width"] == 80


def test_every_stdout_line_is_a_protocol_message_and_stdin_eof_exits_cleanly(engine):
    ask(engine, 1, "engine.ping")
    engine.stdin.close()
    remaining = engine.stdout.read().decode("utf-8")
    assert remaining == ""
    assert engine.wait(10) == 0
    assert "Traceback" not in engine.stderr.read().decode("utf-8", "replace")


def test_stray_prints_do_not_reach_the_protocol_stream(tmp_path):
    """Anything a library writes to stdout, in Python or in native code, goes to stderr."""
    script = tmp_path / "noisy.py"
    script.write_text(
        "import os, sys\n"
        "from opendoc_engine.__main__ import main\n"
        "import opendoc_engine.methods as methods\n"
        "real = methods.build_server\n"
        "def noisy():\n"
        "    print('python print')\n"
        "    os.write(1, b'native write\\n')\n"
        "    return real()\n"
        "methods.build_server = noisy\n"
        "sys.exit(main())\n",
        encoding="utf-8",
    )
    result = subprocess.run(
        [sys.executable, str(script)],
        input=b'{"jsonrpc":"2.0","id":1,"method":"engine.ping"}\n',
        capture_output=True,
        timeout=60,
        check=False,
    )
    assert result.stdout.decode().strip() == '{"jsonrpc":"2.0","id":1,"result":"pong"}'
    assert b"python print" in result.stderr
    assert b"native write" in result.stderr
