"""The engine's client for llama-server's OpenAI-compatible chat endpoint.

Used directly by the Max tier and, through `check_health`, by the Accurate tier. Requests
stream, so cancellation can stop reading at the next token and the connection is closed when
`convert.cancel` arrives. Everything that can fail maps to the protocol's errors:
LLAMA_SERVER_UNREACHABLE (`connection_refused`, `timeout`, `bad_response`), OUT_OF_MEMORY and
RECOGNITION_FAILED.
"""

from __future__ import annotations

import base64
import json
import logging
from dataclasses import dataclass
from typing import Any

import httpx

from opendoc_engine.errors import (
    cancelled,
    llama_server_unreachable,
    looks_like_oom,
    oom_reason,
    out_of_memory,
    recognition_failed,
)
from opendoc_engine.rpc import CallContext

log = logging.getLogger(__name__)

# Generous: prompt processing of a page on a slow CPU takes a while before the first token.
_TIMEOUT = httpx.Timeout(connect=5.0, read=600.0, write=120.0, pool=5.0)
_HEALTH_TIMEOUT = httpx.Timeout(5.0)
# Send a progress notification every this many streamed chunks, as a sign of life.
_PROGRESS_EVERY = 100


@dataclass
class Completion:
    text: str
    finish_reason: str | None
    chunks: int


def _error_text(response: httpx.Response) -> str:
    try:
        body = response.read().decode("utf-8", "replace")
    except httpx.HTTPError:
        return ""
    try:
        message = json.loads(body).get("error", {}).get("message")
    except (ValueError, AttributeError):
        message = None
    return str(message or body)[:500]


def _raise_for_status(response: httpx.Response) -> None:
    if response.status_code == 200:
        return
    text = _error_text(response)
    if looks_like_oom(text):
        raise out_of_memory(oom_reason(text), text)
    if "context" in text.lower() and ("exceed" in text.lower() or "n_ctx" in text.lower()):
        raise recognition_failed("max_tokens", text)
    raise llama_server_unreachable("bad_response", f"HTTP {response.status_code}: {text}")


class LlamaClient:
    def __init__(self, base_url: str) -> None:
        self.base_url = base_url.rstrip("/")

    def check_health(self) -> None:
        """Fail with LLAMA_SERVER_UNREACHABLE unless the server answers `/health` with 200."""
        try:
            response = httpx.get(f"{self.base_url}/health", timeout=_HEALTH_TIMEOUT)
        except httpx.ConnectError as exc:
            raise llama_server_unreachable("connection_refused", self.base_url) from exc
        except httpx.TimeoutException as exc:
            raise llama_server_unreachable("timeout", self.base_url) from exc
        except httpx.HTTPError as exc:
            raise llama_server_unreachable("bad_response", f"{self.base_url}: {exc}") from exc
        if response.status_code != 200:
            raise llama_server_unreachable(
                "bad_response", f"{self.base_url}/health answered {response.status_code}"
            )

    def complete_image(
        self,
        *,
        png: bytes,
        prompt: str,
        max_tokens: int,
        call: CallContext,
        extra: dict[str, Any] | None = None,
    ) -> Completion:
        """Send one page image with `prompt` and collect the streamed answer. `extra` is merged
        into the request body (sampling parameters)."""
        image_url = "data:image/png;base64," + base64.b64encode(png).decode("ascii")
        body: dict[str, Any] = {
            "model": "model",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "image_url", "image_url": {"url": image_url}},
                        {"type": "text", "text": prompt},
                    ],
                }
            ],
            "temperature": 0,
            "max_tokens": max_tokens,
            "stream": True,
            **(extra or {}),
        }
        client = httpx.Client(timeout=_TIMEOUT)
        pieces: list[str] = []
        finish: str | None = None
        chunks = 0
        try:
            with call.on_cancel(client.close):
                call.check_cancelled()
                with client.stream(
                    "POST", f"{self.base_url}/v1/chat/completions", json=body
                ) as response:
                    _raise_for_status(response)
                    for line in response.iter_lines():
                        if call.cancelled.is_set():
                            raise cancelled()
                        if not line.startswith("data:"):
                            continue
                        payload = line[5:].strip()
                        if payload == "[DONE]":
                            break
                        text, reason = _parse_chunk(payload)
                        if text:
                            pieces.append(text)
                        finish = reason or finish
                        chunks += 1
                        if chunks % _PROGRESS_EVERY == 0:
                            call.stage("recognize", chunks)
        except httpx.ConnectError as exc:
            raise llama_server_unreachable("connection_refused", self.base_url) from exc
        except httpx.TimeoutException as exc:
            raise llama_server_unreachable("timeout", self.base_url) from exc
        except httpx.HTTPError as exc:
            if call.cancelled.is_set():
                raise cancelled() from exc
            raise llama_server_unreachable("bad_response", f"{self.base_url}: {exc}") from exc
        except (RuntimeError, OSError) as exc:
            # httpx raises RuntimeError when the client is closed under a running request.
            if call.cancelled.is_set():
                raise cancelled() from exc
            raise
        finally:
            client.close()
        return Completion("".join(pieces), finish, chunks)


def _parse_chunk(payload: str) -> tuple[str, str | None]:
    try:
        data = json.loads(payload)
        choice = data["choices"][0]
    except (ValueError, KeyError, IndexError, TypeError) as exc:
        raise llama_server_unreachable(
            "bad_response", f"unexpected stream data: {payload[:200]}"
        ) from exc
    delta = choice.get("delta") or {}
    return delta.get("content") or "", choice.get("finish_reason")
