"""The llama-server client against a small fake server that speaks the chat streaming format."""

import json
import socket
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from opendoc_engine.llama import LlamaClient
from opendoc_engine.rpc import CallContext, RpcError


class Fake:
    """Behaviour of the fake server, changed per test."""

    def __init__(self) -> None:
        self.health = (200, {"status": "ok"})
        self.status = 200
        self.error: dict | None = None
        self.pieces = ["Hello", " world"]
        self.finish = "stop"
        self.delay = 0.0
        self.requests: list[dict] = []


@pytest.fixture
def fake():
    state = Fake()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_GET(self):
            code, body = state.health
            self._json(code, body)

        def do_POST(self):
            length = int(self.headers["Content-Length"])
            state.requests.append(json.loads(self.rfile.read(length)))
            if state.status != 200:
                self._json(state.status, {"error": state.error})
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            try:
                for piece in state.pieces:
                    self._chunk({"choices": [{"delta": {"content": piece}, "finish_reason": None}]})
                    time.sleep(state.delay)
                self._chunk({"choices": [{"delta": {}, "finish_reason": state.finish}]})
                self.wfile.write(b"data: [DONE]\n\n")
                self.wfile.flush()
            except OSError:
                pass  # the client hung up

        def _chunk(self, payload):
            self.wfile.write(b"data: " + json.dumps(payload).encode() + b"\n\n")
            self.wfile.flush()

        def _json(self, code, body):
            data = json.dumps(body).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(
        target=server.serve_forever, kwargs={"poll_interval": 0.02}, daemon=True
    ).start()
    state.url = f"http://127.0.0.1:{server.server_address[1]}"
    yield state
    server.shutdown()
    server.server_close()


def closed_port_url() -> str:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return f"http://127.0.0.1:{sock.getsockname()[1]}"


def complete(url: str, call: CallContext | None = None, **extra):
    return LlamaClient(url).complete_image(
        png=b"\x89PNG data",
        prompt="Read this.",
        max_tokens=77,
        call=call or CallContext(),
        extra=extra or None,
    )


def error_of(url: str, **kwargs) -> RpcError:
    with pytest.raises(RpcError) as caught:
        complete(url, **kwargs)
    return caught.value


def test_streamed_text_is_collected(fake):
    result = complete(fake.url)
    assert (result.text, result.finish_reason) == ("Hello world", "stop")
    assert result.chunks == 3


def test_request_body_carries_the_image_prompt_and_sampling(fake):
    complete(fake.url, dry_multiplier=0.8)
    body = fake.requests[0]
    assert body["stream"] is True
    assert body["temperature"] == 0
    assert body["max_tokens"] == 77
    assert body["dry_multiplier"] == 0.8
    image, text = body["messages"][0]["content"]
    assert image["image_url"]["url"].startswith("data:image/png;base64,")
    assert text == {"type": "text", "text": "Read this."}


def test_the_length_finish_reason_is_reported(fake):
    fake.finish = "length"
    assert complete(fake.url).finish_reason == "length"


def test_a_closed_port_is_connection_refused():
    error = error_of(closed_port_url())
    assert error.code == -32002
    assert error.data["reason"] == "connection_refused"


def test_gpu_out_of_memory_is_reported_as_such(fake):
    fake.status = 500
    fake.error = {"message": "failed to allocate CUDA buffer: out of memory"}
    error = error_of(fake.url)
    assert (error.code, error.data["reason"]) == (-32003, "gpu")


def test_overflowing_the_context_is_recognition_failed(fake):
    fake.status = 400
    fake.error = {"message": "the request exceeds the available context size (n_ctx)"}
    error = error_of(fake.url)
    assert (error.code, error.data["reason"]) == (-32008, "max_tokens")


def test_other_http_errors_are_a_bad_response(fake):
    fake.status = 404
    fake.error = {"message": "File Not Found"}
    error = error_of(fake.url)
    assert (error.code, error.data["reason"]) == (-32002, "bad_response")
    assert "404" in error.data["detail"]


def test_health_check(fake, monkeypatch):
    LlamaClient(fake.url).check_health()
    fake.health = (503, {"error": {"message": "Loading model"}})
    with pytest.raises(RpcError) as loading:
        LlamaClient(fake.url).check_health()
    assert loading.value.data["reason"] == "bad_response"
    with pytest.raises(RpcError) as refused:
        LlamaClient(closed_port_url()).check_health()
    assert refused.value.data["reason"] == "connection_refused"


def test_cancel_stops_a_running_request_promptly(fake):
    fake.pieces = ["x"] * 200
    fake.delay = 0.05  # ten seconds if left alone
    call = CallContext()
    threading.Timer(0.4, call.cancel).start()
    started = time.monotonic()
    error = error_of(fake.url, call=call)
    assert error.code == -32005
    assert time.monotonic() - started < 3


def test_a_request_cancelled_before_it_starts_never_connects(fake):
    call = CallContext()
    call.cancel()
    assert error_of(fake.url, call=call).code == -32005
    assert fake.requests == []
