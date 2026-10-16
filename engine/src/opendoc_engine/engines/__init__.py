"""Recognition engines and their registry."""

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
    libraries on first use, so building the registry is cheap."""
    from opendoc_engine.engines.accurate import AccurateEngine
    from opendoc_engine.engines.deepseek import DeepseekEngine
    from opendoc_engine.engines.fast import FastEngine

    registry = EngineRegistry()
    registry.register(FastEngine())
    registry.register(AccurateEngine())
    registry.register(DeepseekEngine())
    registry.register(DemoEngine())
    return registry
