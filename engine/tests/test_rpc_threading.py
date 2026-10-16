"""The concurrency rules of docs/protocol.md 1.2, with a slow method standing in for a page."""

import json
import queue
import threading
import time

import pytest

from opendoc_engine.engines import default_registry
from opendoc_engine.methods import build_methods
from opendoc_engine.rpc import RpcServer, current_call


class Pipe:
    """A line-oriented stdin and stdout for `RpcServer.serve`, driven from the test."""

    def __init__(self) -> None:
        self._inbox: queue.Queue[str | None] = queue.Queue()
        self.outbox: queue.Queue[dict] = queue.Queue()

    # reader side (what serve iterates)
    def __iter__(self):
        while (line := self._inbox.get()) is not None:
            yield line

    # writer side (what serve writes to)
    def write(self, text: str) -> None:
        for line in text.splitlines():
            self.outbox.put(json.loads(line))

    def flush(self) -> None:
        pass

    def send(self, message: dict | str) -> None:
        self._inbox.put(message if isinstance(message, str) else json.dumps(message))

    def close(self) -> None:
        self._inbox.put(None)

    def next(self, timeout: float = 5.0) -> dict:
        return self.outbox.get(timeout=timeout)

    def nothing_for(self, seconds: float) -> bool:
        try:
            self.outbox.get(timeout=seconds)
        except queue.Empty:
            return True
        return False


def request(request_id, method, params=None):
    message = {"jsonrpc": "2.0", "id": request_id, "method": method}
    if params is not None:
        message["params"] = params
    return message


@pytest.fixture
def served():
    """A running server with `test.slow` (waits for a release event or for cancellation) and
    `test.progress` (reports two stages) added to the real method set."""
    methods = build_methods(default_registry())
    release = threading.Event()
    started = threading.Event()
    seen = {"hooks": 0, "released": False}

    def slow(_params):
        call = current_call()
        started.set()
        with call.on_cancel(lambda: seen.__setitem__("hooks", seen["hooks"] + 1)):
            while not release.is_set():
                call.check_cancelled()
                time.sleep(0.01)
        seen["released"] = True
        return "done"

    def progress(_params):
        call = current_call()
        call.page_index = 4
        call.stage("layout")
        call.stage("recognize", 1, 2)
        return "ok"

    methods.register("test.slow", slow)
    methods.register("test.progress", progress)
    shutdown = []
    server = RpcServer(methods, on_shutdown=lambda: shutdown.append(True))
    pipe = Pipe()
    thread = threading.Thread(target=server.serve, args=(pipe, pipe), daemon=True)
    thread.start()
    yield pipe, release, started, seen, shutdown, thread
    release.set()
    pipe.close()
    thread.join(5)


def test_second_request_while_one_runs_is_busy(served):
    pipe, release, started, *_ = served
    pipe.send(request(1, "test.slow"))
    assert started.wait(5)
    pipe.send(request(2, "engine.version"))
    busy = pipe.next()
    assert busy["id"] == 2
    assert busy["error"]["code"] == -32007
    assert busy["error"]["data"] == {"reason": "call_in_flight"}
    release.set()
    assert pipe.next() == {"jsonrpc": "2.0", "id": 1, "result": "done"}


def test_the_next_request_after_a_response_is_never_busy(served):
    pipe, release, *_ = served
    release.set()
    for number in range(1, 40):  # back to back, as fast as the pipe allows
        pipe.send(request(number, "test.slow"))
        response = pipe.next()
        assert response == {"jsonrpc": "2.0", "id": number, "result": "done"}


def test_ping_is_answered_while_a_request_runs(served):
    pipe, release, started, *_ = served
    pipe.send(request(1, "test.slow"))
    assert started.wait(5)
    pipe.send(request(2, "engine.ping"))
    assert pipe.next() == {"jsonrpc": "2.0", "id": 2, "result": "pong"}
    release.set()
    assert pipe.next()["id"] == 1


def test_cancel_stops_the_request_in_flight(served):
    pipe, _release, started, seen, *_ = served
    pipe.send(request(8, "test.slow"))
    assert started.wait(5)
    pipe.send(request(9, "convert.cancel", {"requestId": 8}))
    first, second = pipe.next(), pipe.next()
    by_id = {m["id"]: m for m in (first, second)}
    assert by_id[9]["result"] == {"cancelled": True}
    assert by_id[8]["error"]["code"] == -32005
    assert by_id[8]["error"]["data"] == {"reason": "cancelled"}
    assert seen["hooks"] == 1  # the cancel hook ran
    assert not seen["released"]


def test_cancelling_nothing_is_not_an_error(served):
    pipe, *_ = served
    pipe.send(request(1, "convert.cancel", {"requestId": 99}))
    assert pipe.next() == {"jsonrpc": "2.0", "id": 1, "result": {"cancelled": False}}
    pipe.send(request(2, "convert.cancel", {}))
    assert pipe.next()["error"]["data"]["reason"] == "missing_param"


def test_progress_notifications_come_before_the_response(served):
    pipe, *_ = served
    pipe.send(request(5, "test.progress"))
    messages = [pipe.next(), pipe.next(), pipe.next()]
    assert [m.get("method") for m in messages] == ["convert.progress", "convert.progress", None]
    assert "id" not in messages[0]
    assert messages[0]["params"] == {"requestId": 5, "pageIndex": 4, "stage": "layout"}
    assert messages[1]["params"] == {
        "requestId": 5,
        "pageIndex": 4,
        "stage": "recognize",
        "current": 1,
        "total": 2,
    }
    assert messages[2] == {"jsonrpc": "2.0", "id": 5, "result": "ok"}


def test_malformed_input_is_answered_without_the_worker(served):
    pipe, release, started, *_ = served
    pipe.send(request(1, "test.slow"))
    assert started.wait(5)
    pipe.send("{broken")
    assert pipe.next()["error"]["code"] == -32700
    pipe.send(request(2, "no.such.method"))
    assert pipe.next()["error"]["code"] == -32601
    release.set()
    assert pipe.next()["id"] == 1


def test_a_notification_gets_no_response(served):
    pipe, *_ = served
    pipe.send({"jsonrpc": "2.0", "method": "engine.ping"})
    pipe.send({"jsonrpc": "2.0", "method": "no.such.method"})
    assert pipe.nothing_for(0.3)


def test_closing_stdin_cancels_the_request_and_runs_the_shutdown_hook(served):
    pipe, _release, started, _seen, shutdown, thread = served
    pipe.send(request(1, "test.slow"))
    assert started.wait(5)
    pipe.close()
    thread.join(5)
    assert not thread.is_alive()
    assert shutdown == [True]
    assert pipe.next()["error"]["code"] == -32005
