"""The Max engine, with llama-server replaced by answers recorded from DeepSeek-OCR v1."""

from pathlib import Path
from typing import ClassVar

import pytest

from opendoc_engine.engines import EngineSetup
from opendoc_engine.engines.deepseek import (
    MAX_TOKENS,
    PROMPT,
    DeepseekEngine,
    uses_dry_sampling,
)
from opendoc_engine.ir import BlockType
from opendoc_engine.llama import Completion
from opendoc_engine.rpc import CallContext, RpcError

FIXTURES = Path(__file__).parent / "fixtures"
SIZE = (1318, 2200)


def answer(name: str) -> str:
    return (FIXTURES / f"deepseek-{name}.txt").read_text(encoding="utf-8")


class FakeClient:
    """Stands in for LlamaClient and records the request."""

    requests: ClassVar[list[dict]] = []
    reply = Completion("", "stop", 0)

    def __init__(self, base_url: str) -> None:
        self.base_url = base_url

    def complete_image(self, **kwargs) -> Completion:
        FakeClient.requests.append({"base_url": self.base_url, **kwargs})
        return FakeClient.reply


@pytest.fixture
def engine(monkeypatch, tmp_path):
    monkeypatch.setattr("opendoc_engine.engines.deepseek.LlamaClient", FakeClient)
    FakeClient.requests = []
    FakeClient.reply = Completion("", "stop", 0)
    image = tmp_path / "page.png"
    image.write_bytes(b"\x89PNG fake")
    instance = DeepseekEngine()
    instance.prepare(setup("max-deepseek-ocr-1-q8", tmp_path))
    instance.image = image
    return instance


def setup(model_id: str, tmp_path: Path) -> EngineSetup:
    return EngineSetup(model_id, tmp_path, "http://127.0.0.1:9", 4, tmp_path / "_engine")


def run(engine: DeepseekEngine, text: str, finish: str = "stop"):
    FakeClient.reply = Completion(text, finish, 10)
    return engine.run(engine.image, SIZE, CallContext())


def test_request_uses_the_grounding_prompt_and_no_dry_for_v1(engine):
    run(engine, answer("book"))
    request = FakeClient.requests[0]
    assert request["prompt"] == PROMPT == "<|grounding|>Convert the document to markdown."
    assert request["max_tokens"] == MAX_TOKENS
    assert request["extra"] is None
    assert request["base_url"] == "http://127.0.0.1:9"


def test_the_experimental_model_sends_dry_sampling(engine, tmp_path):
    assert uses_dry_sampling("experimental-deepseek-ocr-2-q8")
    assert not uses_dry_sampling("max-deepseek-ocr-1-q8")
    engine.prepare(setup("experimental-deepseek-ocr-2-q8", tmp_path))
    run(engine, answer("book"))
    extra = FakeClient.requests[0]["extra"]
    assert extra["dry_multiplier"] == 0.8
    assert extra["dry_base"] == 1.75
    assert extra["dry_allowed_length"] == 2
    assert extra["dry_penalty_last_n"] == 8192


def test_book_page_blocks(engine):
    blocks = run(engine, answer("book"))
    kinds = [b.type for b in blocks]
    assert kinds.count(BlockType.PARAGRAPH) == 4
    assert BlockType.FIGURE in kinds
    caption = next(b for b in blocks if b.type is BlockType.CAPTION)
    assert "<center>" not in caption.text
    assert caption.caption_for == next(b.id for b in blocks if b.type is BlockType.FIGURE)


def test_table_page_has_a_table_with_spans(engine):
    blocks = run(engine, answer("table"))
    table = next(b for b in blocks if b.type is BlockType.TABLE).table
    assert table.has_merged_cells
    spans = {(c.row, c.col): (c.row_span, c.col_span) for c in table.cells}
    assert spans[(0, 0)] == (2, 1)
    assert spans[(0, 1)] == (1, 3)


def test_math_page_formulas_get_their_printed_numbers(engine):
    blocks = run(engine, answer("math"))
    formulas = [b for b in blocks if b.type is BlockType.FORMULA]
    assert [b.number for b in formulas if b.number] == ["(2)", "(3)", "(4)"]
    assert all("\\(" not in (b.text or "") for b in blocks)  # inline math uses dollars
    assert any("$P_1(x_1, y_1)$" in (b.text or "") for b in blocks)


def test_a_loop_that_hits_the_token_limit_is_recognition_failed(engine):
    with pytest.raises(RpcError) as caught:
        run(engine, "<|ref|>text<|/ref|><|det|>[[0,0,9,9]]<|/det|>\nloop " * 5, finish="length")
    assert caught.value.code == -32008
    assert caught.value.data["reason"] == "max_tokens"


@pytest.mark.parametrize("text", ["", "   \n"])
def test_empty_answer_is_recognition_failed(engine, text):
    with pytest.raises(RpcError) as caught:
        run(engine, text)
    assert caught.value.data["reason"] == "output_unusable"


def test_run_before_prepare_is_not_configured():
    with pytest.raises(RpcError) as caught:
        DeepseekEngine().run("x.png", SIZE)
    assert caught.value.code == -32006


def test_prepare_without_a_server_url_is_not_configured(tmp_path):
    bare = EngineSetup("max-deepseek-ocr-1-q8", tmp_path, None, 4, tmp_path)
    with pytest.raises(RpcError) as caught:
        DeepseekEngine().prepare(bare)
    assert caught.value.data["reason"] == "llama_server_url"
