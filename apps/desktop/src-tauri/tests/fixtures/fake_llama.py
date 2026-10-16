"""A stand-in for llama-server: answers /health on --port.

  FAKE_LLAMA_LOADING_MS   answer 503 for this long, as the real server does while it loads
  FAKE_LLAMA_EXIT         exit with this status right after start (a crash on load)
"""

import os
import sys
import time
import socketserver
from http.server import BaseHTTPRequestHandler, HTTPServer

args = sys.argv[1:]
port = int(args[args.index("--port") + 1])

if os.environ.get("FAKE_LLAMA_EXIT"):
    sys.stderr.write("error: failed to load model\n")
    sys.exit(int(os.environ["FAKE_LLAMA_EXIT"]))

started = time.time()
loading = int(os.environ.get("FAKE_LLAMA_LOADING_MS", "0")) / 1000.0


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):  # noqa: N802
        ready = time.time() - started >= loading
        body = b'{"status":"ok"}' if ready else b'{"error":"loading"}'
        self.send_response(200 if ready else 503)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


sys.stderr.write("fake llama-server listening\n")
sys.stderr.flush()
class Server(HTTPServer):
    # HTTPServer.server_bind looks up the host's fully qualified name, which can
    # stall for tens of seconds on macOS CI runners. The name is never used here.
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name = "127.0.0.1"
        self.server_port = self.server_address[1]


Server(("127.0.0.1", port), Handler).serve_forever()
