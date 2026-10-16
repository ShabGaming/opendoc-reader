"""Recognition engines and their registry."""

import os

from opendoc_engine.engines.base import (
    Capabilities,
    Engine,
    EngineRegistry,
    EngineSetup,
    describe,
)
from opendoc_engine.engines.demo import DemoEngine

__all__ = [
    "Capabilities",
    "DemoEngine",
    "Engine",
    "EngineRegistry",
    "EngineSetup",
    "default_registry",
    "describe",
]


def default_registry() -> EngineRegistry:
    """The engines available in this build. The Paddle and DeepSeek engines import their heavy
    libraries on first use, so building the registry is cheap. The demo engine is a test fixture
    and is only offered when OPENDOC_DEMO_ENGINE=1."""
    from opendoc_engine.engines.accurate import AccurateEngine
    from opendoc_engine.engines.deepseek import DeepseekEngine
    from opendoc_engine.engines.fast import FastEngine

    registry = EngineRegistry()
    registry.register(FastEngine())
    registry.register(AccurateEngine())
    registry.register(DeepseekEngine())
    if os.environ.get("OPENDOC_DEMO_ENGINE") == "1":
        registry.register(DemoEngine())
    return registry
