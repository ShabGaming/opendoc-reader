"""A stand-in for the Python engine that speaks docs/protocol.md, for the Rust tests.

It answers every method of the protocol with plausible, deterministic results and writes the
files the real engine would (page PNGs, IR pages, the Document, the exports). Behaviour is
steered by environment variables so tests can provoke failures:

  FAKE_LOG                  append one JSON line per request (method and params) to this file
  FAKE_PAGES                page count reported for PDFs (default 3)
  FAKE_TEXT_LAYER           "1" to report a text layer on every PDF page
  FAKE_SLOW_MS              time convert.page takes, in steps that check for convert.cancel
  FAKE_FAIL_PAGE            convert.page for this page index fails with FAKE_FAIL_CODE
  FAKE_FAIL_CODE            JSON-RPC error code for that failure (default -32008)
  FAKE_FAIL_REASON          its data.reason (default output_unusable)
  FAKE_CRASH_ONCE_ON        exit abruptly the first time this method is called
  FAKE_STATE_DIR            where the crash marker is kept (survives restarts)
  FAKE_HANG_ON              never answer this method (and send no progress)
  FAKE_LLAMA_UNREACHABLE    convert.page answers LLAMA_SERVER_UNREACHABLE once
  FAKE_INSPECT_FAIL         pdf.inspect and image.inspect fail with BAD_INPUT and this reason
  FAKE_FIGURE               "1" to have export.markdown write an asset file
"""

import json
import os
import struct
import sys
import threading
import time
import zlib
from pathlib import Path

write_lock = threading.Lock()
busy_lock = threading.Lock()
cancel_events = {}
state = {"configured": None, "busy": False}


def send(message):
    with write_lock:
        sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
        sys.stdout.flush()


def error(request_id, code, message, data=None):
    body = {"code": code, "message": message}
    if data is not None:
        body["data"] = data
    send({"jsonrpc": "2.0", "id": request_id, "error": body})


class Rpc(Exception):
    def __init__(self, code, message, data=None):
        self.code, self.message, self.data = code, message, data


def log_request(method, params):
    path = os.environ.get("FAKE_LOG")
    if path:
        with open(path, "a", encoding="utf-8") as handle:
            handle.write(json.dumps({"method": method, "params": params}) + "\n")


def png_bytes(width=8, height=8):
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + b"\xff" * (width * 3) for _ in range(height))
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def write_atomic(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name(path.name + ".tmp")
    temp.write_bytes(data if isinstance(data, bytes) else data.encode("utf-8"))
    os.replace(temp, path)


def maybe_crash(method):
    if os.environ.get("FAKE_CRASH_ONCE_ON") == method:
        marker = Path(os.environ.get("FAKE_STATE_DIR", ".")) / "crashed"
        if not marker.exists():
            marker.write_text("1")
            os._exit(7)


def page_count_for(path):
    return int(os.environ.get("FAKE_PAGES", "3"))


def engine_info(engine_id, model):
    return {"id": engine_id, "model": model, "version": "0.1.0"}


def block(page_index, n, text):
    return {
        "id": f"p{page_index}-b{n}",
        "type": "paragraph",
        "bbox": [10.0, 10.0 + n * 20, 90.0, 25.0 + n * 20],
        "order": n,
        "text": text,
    }


def write_page_ir(params, engine_id, model):
    page_index = params["pageIndex"]
    page = {
        "index": page_index,
        "width": 8,
        "height": 8,
        "sourceImage": params["imagePath"],
        "blocks": [block(page_index, 0, f"Page {page_index + 1}")],
    }
    write_atomic(params["irPath"], json.dumps(page))
    return {
        "irPath": params["irPath"],
        "blockCount": 1,
        "engine": engine_info(engine_id, model),
        "elapsedMs": 5,
        "warnings": [],
    }


def convert_page(request_id, params):
    page_index = params["pageIndex"]
    cancel = cancel_events.setdefault(request_id, threading.Event())
    send({"jsonrpc": "2.0", "method": "convert.progress",
          "params": {"requestId": request_id, "pageIndex": page_index, "stage": "prepare"}})
    if os.environ.get("FAKE_LLAMA_UNREACHABLE"):
        marker = Path(os.environ.get("FAKE_STATE_DIR", ".")) / "llama-failed"
        if not marker.exists():
            marker.write_text("1")
            raise Rpc(-32002, "llama-server unreachable", {"reason": "connection_refused"})
    slow = int(os.environ.get("FAKE_SLOW_MS", "0"))
    waited = 0
    while waited < slow:
        if cancel.is_set():
            raise Rpc(-32005, "Cancelled", {"reason": "cancelled"})
        send({"jsonrpc": "2.0", "method": "convert.progress",
              "params": {"requestId": request_id, "pageIndex": page_index, "stage": "recognize",
                         "current": waited, "total": slow}})
        time.sleep(0.05)
        waited += 50
    if cancel.is_set():
        raise Rpc(-32005, "Cancelled", {"reason": "cancelled"})
    fail_page = os.environ.get("FAKE_FAIL_PAGE")
    if fail_page is not None and int(fail_page) == page_index:
        raise Rpc(
            int(os.environ.get("FAKE_FAIL_CODE", "-32008")),
            "Recognition failed",
            {"reason": os.environ.get("FAKE_FAIL_REASON", "output_unusable")},
        )
    return write_page_ir(params, params["engineId"], params["modelId"])


def document_assemble(params):
    pages = []
    for path in params["pages"]:
        pages.append(json.loads(Path(path).read_text(encoding="utf-8")))
    document = {
        "schemaVersion": "1",
        "source": params["source"],
        "engine": params["engine"],
        "pages": pages,
    }
    write_atomic(params["outPath"], json.dumps(document))
    return {"path": params["outPath"], "pageCount": len(pages)}


def refuse_existing(path):
    if Path(path).exists():
        raise Rpc(-32009, "I/O error", {"reason": "already_exists"})


def export_docx(params):
    refuse_existing(params["outputPath"])
    write_atomic(params["outputPath"], b"fake docx")
    return {"files": [params["outputPath"]], "warnings": []}


def export_markdown(params):
    refuse_existing(params["outputPath"])
    files = [params["outputPath"]]
    write_atomic(params["outputPath"], "# Fake\n")
    if os.environ.get("FAKE_FIGURE") == "1":
        asset = Path(params["assetsDir"]) / "p0-b0.png"
        refuse_existing(asset)
        write_atomic(asset, png_bytes())
        files.append(str(asset))
    return {"files": files, "warnings": []}


def handle(request_id, method, params):
    maybe_crash(method)
    if os.environ.get("FAKE_HANG_ON") == method:
        time.sleep(3600)
    if method == "engine.ping":
        return "pong"
    if method == "engine.version":
        return {"engine": "0.1.0", "irSchema": "1", "protocol": "1"}
    if method == "engine.capabilities":
        capabilities = {"tables": False, "formulas": False, "figures": False,
                        "readingOrder": True, "boundingBoxes": True, "languageHandling": "none"}
        return [{"id": "demo", "displayName": "Fake", "capabilities": capabilities,
                 "manifestModels": []}]
    if method == "engine.configure":
        state["configured"] = params
        return {"cpuThreads": params.get("cpuThreads") or 4}
    if method in ("pdf.inspect", "image.inspect"):
        if os.environ.get("FAKE_INSPECT_FAIL"):
            raise Rpc(-32004, "Bad input", {"reason": os.environ["FAKE_INSPECT_FAIL"]})
        count = page_count_for(params["path"]) if method == "pdf.inspect" else 1
        has_text = os.environ.get("FAKE_TEXT_LAYER") == "1"
        pages = []
        for index in range(count):
            page = {"index": index, "width": 595.0 if method == "pdf.inspect" else 8, "height": 842.0 if method == "pdf.inspect" else 8}
            if method == "pdf.inspect":
                page["hasTextLayer"] = has_text
                page["charCount"] = 2000 if has_text else 0
            pages.append(page)
        return {"pageCount": count, "pages": pages}
    if method == "page.render":
        write_atomic(params["outPath"], png_bytes())
        return {"path": params["outPath"], "width": 8, "height": 8,
                "dpi": 200, "rotation": 0, "skewAngle": 0}
    if method == "convert.page":
        return convert_page(request_id, params)
    if method == "text.extract":
        return write_page_ir(params, "pdf-text", "text-layer")
    if method == "document.assemble":
        return document_assemble(params)
    if method == "export.docx":
        return export_docx(params)
    if method == "export.markdown":
        return export_markdown(params)
    if method == "calibrate.run":
        send({"jsonrpc": "2.0", "method": "convert.progress",
              "params": {"requestId": request_id, "pageIndex": 0, "stage": "recognize"}})
        return {"secondsPerPage": 0.5, "warmupSeconds": 1.0, "runSeconds": [0.5, 0.5]}
    raise Rpc(-32601, f"Method not found: {method}")


def run_worker(request_id, method, params):
    # The busy flag is cleared before the reply goes out, so the next request cannot race it.
    try:
        try:
            result = handle(request_id, method, params)
        finally:
            cancel_events.pop(request_id, None)
        state["busy"] = False
        send({"jsonrpc": "2.0", "id": request_id, "result": result})
    except Rpc as exc:
        state["busy"] = False
        error(request_id, exc.code, exc.message, exc.data)
    except Exception as exc:  # noqa: BLE001
        state["busy"] = False
        sys.stderr.write(f"fake engine failure: {exc!r}\n")
        error(request_id, -32603, "Internal error")


def main():
    for raw in sys.stdin:
        line = raw.strip()
        if not line:
            continue
        message = json.loads(line)
        request_id = message.get("id")
        method = message["method"]
        params = message.get("params") or {}
        log_request(method, params)
        if method == "convert.cancel":
            target = params["requestId"]
            event = cancel_events.get(target)
            if event is not None:
                event.set()
            send({"jsonrpc": "2.0", "id": request_id, "result": {"cancelled": event is not None}})
            continue
        if method == "engine.ping":
            send({"jsonrpc": "2.0", "id": request_id, "result": "pong"})
            continue
        with busy_lock:
            if state["busy"]:
                error(request_id, -32007, "Engine busy", {"reason": "call_in_flight"})
                continue
            state["busy"] = True
        cancel_events[request_id] = threading.Event()
        threading.Thread(target=run_worker, args=(request_id, method, params), daemon=True).start()


if __name__ == "__main__":
    main()
