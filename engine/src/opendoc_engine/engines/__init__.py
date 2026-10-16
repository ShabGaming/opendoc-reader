"""Recognition engines and their registry."""

from opendoc_engine.engines.base import Capabilities, Engine, EngineRegistry, describe
from opendoc_engine.engines.demo import DemoEngine

__all__ = ["Capabilities", "DemoEngine", "Engine", "EngineRegistry", "default_registry", "describe"]


def default_registry() -> EngineRegistry:
    """The engines available in this build. Real engines are added here as they land."""
    registry = EngineRegistry()
    registry.register(DemoEngine())
    return registry
