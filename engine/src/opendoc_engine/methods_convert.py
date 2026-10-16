"""RPC methods for recognition: `engine.configure`, `convert.page`, `convert.cancel` and
`calibrate.run` (docs/protocol.md 1.5)."""

from __future__ import annotations

import shutil
import statistics
import time
from pathlib import Path
from typing import Any

from opendoc_engine import preprocess
from opendoc_engine.engines import EngineRegistry
from opendoc_engine.params import Field, parse_params
from opendoc_engine.pipeline import convert_page
from opendoc_engine.rpc import MethodRegistry, current_call
from opendoc_engine.runtime import Runtime

_CONFIGURE = {
    "llamaServerUrl": Field("nullable_str"),
    "workDir": Field("path"),
    "modelsDir": Field("path"),
    "cpuThreads": Field("int", required=False, minimum=1),
}
_CONVERT = {
    "engineId": Field("str"),
    "modelId": Field("str"),
    "imagePath": Field("path"),
    "pageIndex": Field("int", minimum=0),
    "irPath": Field("path"),
    "cropsDir": Field("path"),
    "options": Field("object", required=False, default={}),
}
_OPTIONS = {"language": Field("str", required=False)}
_CANCEL = {"requestId": Field("id")}
_CALIBRATE = {
    "engineId": Field("str"),
    "modelId": Field("str"),
    "samplePath": Field("path"),
    "runs": Field("int", required=False, default=2, minimum=1, maximum=5),
}


def register(methods: MethodRegistry, engines: EngineRegistry, runtime: Runtime) -> None:
    @methods.method("engine.configure")
    def configure(params: Any) -> dict[str, int]:
        p = parse_params(params, _CONFIGURE)
        settings = runtime.configure(
            llama_server_url=p["llamaServerUrl"],
            work_dir=p["workDir"],
            models_dir=p["modelsDir"],
            cpu_threads=p["cpuThreads"],
        )
        # Whatever was loaded may depend on the old URL, folders or thread count.
        engines.release_all()
        preprocess.release_models()
        return {"cpuThreads": settings.cpu_threads}

    @methods.method("convert.page")
    def convert(params: Any) -> dict[str, Any]:
        p = parse_params(params, _CONVERT)
        options = parse_params(p["options"], _OPTIONS)
        result = convert_page(
            engines,
            runtime.settings(),
            engine_id=p["engineId"],
            model_id=p["modelId"],
            image_path=p["imagePath"],
            page_index=p["pageIndex"],
            ir_path=p["irPath"],
            crops_dir=p["cropsDir"],
            options=options,
            call=current_call(),
        )
        return result.to_dict()

    @methods.method("convert.cancel", immediate=True)
    def cancel(params: Any) -> dict[str, bool]:
        p = parse_params(params, _CANCEL)
        return {"cancelled": current_call().cancel_request(p["requestId"])}

    @methods.method("calibrate.run")
    def calibrate(params: Any) -> dict[str, Any]:
        p = parse_params(params, _CALIBRATE)
        settings = runtime.settings()
        call = current_call()
        folder = settings.scratch_dir / "calibration"

        def one_run() -> float:
            call.warnings.clear()
            started = time.monotonic()
            convert_page(
                engines,
                settings,
                engine_id=p["engineId"],
                model_id=p["modelId"],
                image_path=p["samplePath"],
                page_index=0,
                ir_path=str(folder / "page.json"),
                crops_dir=str(folder / "crops"),
                options={},
                call=call,
            )
            return time.monotonic() - started

        warmup = one_run()
        runs = [one_run() for _ in range(p["runs"])]
        _clean(folder)
        return {
            "secondsPerPage": round(statistics.median(runs), 2),
            "warmupSeconds": round(warmup, 2),
            "runSeconds": [round(seconds, 2) for seconds in runs],
        }


def _clean(folder: Path) -> None:
    """Remove the calibration output; it is scratch."""
    shutil.rmtree(folder, ignore_errors=True)
