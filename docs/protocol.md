# Protocol

> Status: v0.1 contract (protocol version 1, IR schema version 1). Three lanes build against this document without talking to each other: the Rust core (M1), the Python engine (M2, M3) and the React UI (M4). If you need something that is not here, change this document, the schema and the examples in the same pull request, so the other lanes see it.

This document has two parts. Part 1 is the JSON-RPC protocol between the Rust core and the Python engine. Part 2 is the command and event contract between the React UI and the Rust core. Both sit on top of the IR, which is specified by [`engine/schema/ir.schema.json`](../engine/schema/ir.schema.json). The design behind them is in [architecture.md](architecture.md).

```
+------------------+  Part 2: Tauri commands (invoke)   +--------------------+
|  React UI        | ---------------------------------> |  Rust core         |
|  (webview)       | <--------------------------------- |  queue, processes, |
|                  |  events: queue://progress,         |  downloads, probe, |
+------------------+          setup://progress          |  settings          |
                                                        +----+----------+----+
                                  Part 1: JSON-RPC 2.0,          |          |
                                  one JSON object per line       |          | starts, stops,
                                  on stdin and stdout            |          | passes the port
                                                                 v          v
                                                        +------------+   +--------------+
                                                        | Python     |   | llama-server |
                                                        | engine     |-->| 127.0.0.1    |
                                                        | (sidecar)  |   | (HTTP)       |
                                                        +------------+   +--------------+
                                                          engine is the only client of
                                                          llama-server; Rust never calls it
```

Three rules shape everything below:

- **Rust owns the queue and the processes.** The engine is stateless between calls and handles one page per call. It does not know about files in the queue, resume, or output naming.
- **The engine is the only client of llama-server.** Rust starts it, picks the port, and tells the engine the URL with `engine.configure`.
- **Messages carry paths, not data.** Page images, IR and exports are files. RPC messages stay small.

---

# Part 1. Engine JSON-RPC

## 1.1 Transport

JSON-RPC 2.0, UTF-8, one JSON object per line (no embedded newlines), over the engine's stdin (requests and notifications from Rust) and stdout (responses and notifications from the engine). The engine opens no port.

- Rust is the only client. Request ids are integers, unique per engine process, and never reused.
- stdout carries protocol messages and nothing else. Anything diagnostic goes to stderr, which Rust copies into the app log. Libraries that print to stdout (Paddle does) must be redirected to stderr before they are imported.
- Rust sends single requests, never batches. The engine accepts batches because the base server does, but nothing depends on it.
- A request with no `id` is a notification and gets no response. The only notification the engine sends is `convert.progress`. Rust sends none.
- `params` is always an object (or omitted, for methods without parameters). Optional parameters have the defaults stated below. Unknown parameters are an `INVALID_PARAMS` error, so a typo does not pass silently.

## 1.2 Process lifecycle and concurrency

**Start.** Rust starts `python -m opendoc_engine` from the engine environment. Importing the heavy libraries is lazy (on first use of an engine), so `engine.ping` answers within a second or two. Startup sequence: `engine.ping` until `"pong"`, then `engine.version` (Rust refuses an engine whose `protocol` is not `"1"`), `engine.capabilities`, `engine.configure`.

**Stop.** Rust closes the engine's stdin. The engine finishes nothing new, releases its models and exits with status 0. Rust kills the process after 5 seconds if it is still alive. If the engine dies or its stdout closes, every request in flight is lost; Rust restarts it, calls `engine.configure` again and retries the page once.

**One call at a time.** Rust sends the next request only after the response to the previous one, with two exceptions that may be sent at any time: `convert.cancel` and `engine.ping`. A second request sent while one is in flight is answered with `BUSY`. For this to work the engine must read stdin on its own thread, run each request on a worker, and answer `convert.cancel` and `engine.ping` immediately. (The current `RpcServer.serve` loop is sequential and is changed in M2-7.)

**Cancellation.** `convert.cancel` names the request id to stop. The engine checks for it between stages and between llama-server requests, aborts any HTTP request it has open, and answers the cancelled request with `CANCELLED`. It writes no IR file for a cancelled page. llama-server may keep generating until it notices the closed connection; Rust restarts it if it needs the GPU back at once.

**Timeouts.** The protocol has none. Rust decides. A suggested policy: treat the engine as stalled when neither a response nor a `convert.progress` notification has arrived for 5 minutes, then kill it and restart as above.

**Statelessness.** The engine may cache loaded models and a llama-server connection between calls, and drops them on `engine.configure`. It keeps nothing else: every call carries all the paths and ids it needs.

## 1.3 Where data lives

Large data never travels in RPC messages. Page images, IR and exports are files, and every call that writes one takes its output path as a parameter, so the engine never decides where things go. All paths are absolute, in the native format of the host OS (on Windows the engine accepts either separator). The engine never rewrites a path it was given.

Rust lays out one job directory per queued file under the work directory it passes to `engine.configure`:

```
<workDir>/<fileId>/
  pages/page-0003.png      page.render output (200 DPI, rotated and deskewed)
  ir/page-0003.json        convert.page or text.extract output: one IR Page
  crops/p3-b5.png          figure crops written by convert.page
  document.json            document.assemble output: the Document IR, input of the exports
<workDir>/_engine/         the engine's own scratch space; Rust does not touch it
```

Page numbers in file names are the 0-based page index, zero-padded to four digits. This layout is Rust's convention, not something the engine checks; the engine only sees the paths. Rust resumes an interrupted file by skipping every page whose IR file exists and parses.

**IR files** follow [`ir.schema.json`](../engine/schema/ir.schema.json). `convert.page` and `text.extract` write a single `Page` object. `document.assemble` writes a `Document`. All paths inside the IR (`sourceImage`, `figure.crop`) are absolute. The engine writes every file atomically (temporary file in the same directory, then rename), so a crash never leaves a half-written file that looks valid.

**Models** live at `<modelsDir>/<modelId>/[<dir>/]<name>`, where `modelId`, `dir` and `name` come from the manifest (`models.<modelId>.files[]`). The downloader writes them there and the engine reads them from there. `modelsDir` is passed in `engine.configure`.

**Outputs next to the original.** The exports write to the paths Rust gives them. Rust picks names that are free (adding a " (2)" suffix when needed) and the engine refuses to overwrite: it fails with `IO_ERROR` and reason `already_exists` instead. The Markdown assets folder is a sibling of the `.md` file, for example `invoice.md` and `invoice_assets/`, and links in the Markdown are relative to the `.md` file.

## 1.4 Errors

Failures are JSON-RPC error responses. `code` and `data.reason` are for programs, `message` is fixed text for the log, and `data.detail` carries an exception message or a path for the log. None of the three is shown to users as is; the UI writes its own copy from the code and reason.

| Code | Name | Meaning | `data.reason` values |
| --- | --- | --- | --- |
| -32700 | PARSE_ERROR | The line was not valid JSON | |
| -32600 | INVALID_REQUEST | Not a valid JSON-RPC request | |
| -32601 | METHOD_NOT_FOUND | Unknown method | |
| -32602 | INVALID_PARAMS | Missing, unknown or malformed parameter, or an unknown `engineId` | `missing_param`, `unknown_param`, `bad_type`, `unknown_engine` (with `data.field` where it applies) |
| -32603 | INTERNAL_ERROR | Unexpected failure inside the engine. The message is always "Internal error"; details go to stderr | |
| -32001 | MODEL_NOT_READY | The model's files are missing or it failed to load | `files_missing`, `load_failed` |
| -32002 | LLAMA_SERVER_UNREACHABLE | A request to llama-server failed at the connection level, timed out, or got an answer that is not llama-server's | `connection_refused`, `timeout`, `bad_response` |
| -32003 | OUT_OF_MEMORY | The model or the page needs more memory than is available | `gpu`, `ram` |
| -32004 | BAD_INPUT | The input file cannot be used | `file_not_found`, `unreadable`, `corrupt`, `password_protected`, `unsupported_format`, `page_out_of_range`, `image_mismatch` |
| -32005 | CANCELLED | The call was stopped by `convert.cancel` | `cancelled` |
| -32006 | NOT_CONFIGURED | A method that needs `engine.configure` (or a llama-server URL) was called before it | `not_configured`, `llama_server_url` |
| -32007 | BUSY | Another call is in flight | `call_in_flight` |
| -32008 | RECOGNITION_FAILED | The model ran but its output is unusable (empty, no parsable blocks, or a repetition loop that hit the token limit) | `output_unusable`, `max_tokens` |
| -32009 | IO_ERROR | Writing an output failed | `already_exists`, `disk_full`, `permission_denied`, `write_failed` |

The codes are constants in `engine/src/opendoc_engine/rpc.py` and an enum in [`engine/schema/rpc/error.schema.json`](../engine/schema/rpc/error.schema.json); a test keeps the three in step. Codes -32010 to -32099 are reserved for future application errors.

Examples of the application errors:

`INVALID_PARAMS`

<!-- example: error.invalid_params -->
```json
{
  "jsonrpc": "2.0",
  "id": 15,
  "error": {
    "code": -32602,
    "message": "Invalid params",
    "data": {"reason": "missing_param", "field": "pageIndex"}
  }
}
```

`MODEL_NOT_READY`

<!-- example: error.model_not_ready -->
```json
{
  "jsonrpc": "2.0",
  "id": 16,
  "error": {
    "code": -32001,
    "message": "Model not ready",
    "data": {
      "reason": "files_missing",
      "detail": "G:\\OpenDoc Reader\\models\\accurate-paddleocr-vl-1.6\\PP-DocLayoutV3\\inference.json"
    }
  }
}
```

`LLAMA_SERVER_UNREACHABLE`

<!-- example: error.llama_server_unreachable -->
```json
{
  "jsonrpc": "2.0",
  "id": 17,
  "error": {
    "code": -32002,
    "message": "llama-server unreachable",
    "data": {"reason": "connection_refused", "detail": "http://127.0.0.1:52871"}
  }
}
```

`OUT_OF_MEMORY`

<!-- example: error.out_of_memory -->
```json
{
  "jsonrpc": "2.0",
  "id": 18,
  "error": {"code": -32003, "message": "Out of memory", "data": {"reason": "gpu"}}
}
```

`BAD_INPUT`

<!-- example: error.bad_input -->
```json
{
  "jsonrpc": "2.0",
  "id": 19,
  "error": {"code": -32004, "message": "Bad input", "data": {"reason": "password_protected"}}
}
```

`CANCELLED` (answers the cancelled request, here the `convert.page` call with id 8)

<!-- example: error.cancelled -->
```json
{
  "jsonrpc": "2.0",
  "id": 8,
  "error": {"code": -32005, "message": "Cancelled", "data": {"reason": "cancelled"}}
}
```

`NOT_CONFIGURED`

<!-- example: error.not_configured -->
```json
{
  "jsonrpc": "2.0",
  "id": 20,
  "error": {"code": -32006, "message": "Engine not configured", "data": {"reason": "llama_server_url"}}
}
```

`BUSY`

<!-- example: error.busy -->
```json
{
  "jsonrpc": "2.0",
  "id": 21,
  "error": {"code": -32007, "message": "Engine busy", "data": {"reason": "call_in_flight"}}
}
```

`RECOGNITION_FAILED`

<!-- example: error.recognition_failed -->
```json
{
  "jsonrpc": "2.0",
  "id": 22,
  "error": {"code": -32008, "message": "Recognition failed", "data": {"reason": "max_tokens"}}
}
```

`IO_ERROR`

<!-- example: error.io_error -->
```json
{
  "jsonrpc": "2.0",
  "id": 23,
  "error": {"code": -32009, "message": "I/O error", "data": {"reason": "already_exists"}}
}
```

## 1.5 Methods

Each method has a JSON Schema for its parameters and result in [`engine/schema/rpc/`](../engine/schema/rpc/) (`<method>.schema.json`, with `$defs.params` and `$defs.result`). The tables below describe the same thing for people; if they disagree, the schema wins.

| Method | Purpose | Notifications | Long-running |
| --- | --- | --- | --- |
| `engine.ping` | Liveness check | | no |
| `engine.version` | Engine, IR and protocol versions | | no |
| `engine.capabilities` | The engines this build can run | | no |
| `engine.configure` | llama-server URL, work and models directories, CPU threads | | no |
| `pdf.inspect` | Page count, page sizes, text-layer presence | | no |
| `image.inspect` | Page count and sizes of an image file | | no |
| `page.render` | One PDF or image page to a preprocessed PNG | | yes |
| `convert.page` | One page image to an IR Page | `convert.progress` | yes |
| `text.extract` | One PDF page's text layer to an IR Page | | yes |
| `convert.cancel` | Stop the call in flight | | no |
| `document.assemble` | IR Page files to one Document IR file | | no |
| `export.docx` | Document IR to a Word file | | yes |
| `export.markdown` | Document IR to Markdown and an assets folder | | yes |
| `calibrate.run` | Time a sample page for the speed estimate | `convert.progress` | yes |

"Long-running" means it can take more than a few seconds, and Rust should not hold a lock that blocks cancellation while waiting.

### engine.ping

No parameters. Result: the string `"pong"`.

<!-- example: engine.ping.request -->
```json
{"jsonrpc": "2.0", "id": 1, "method": "engine.ping"}
```

<!-- example: engine.ping.response -->
```json
{"jsonrpc": "2.0", "id": 1, "result": "pong"}
```

### engine.version

No parameters.

| Result field | Type | Meaning |
| --- | --- | --- |
| `engine` | string | Engine package version |
| `irSchema` | `"1"` | IR schema version the engine writes and reads |
| `protocol` | `"1"` | Version of this protocol |

<!-- example: engine.version.request -->
```json
{"jsonrpc": "2.0", "id": 2, "method": "engine.version"}
```

<!-- example: engine.version.response -->
```json
{"jsonrpc": "2.0", "id": 2, "result": {"engine": "0.1.0", "irSchema": "1", "protocol": "1"}}
```

### engine.capabilities

No parameters. Result: an array with one entry per engine in this build.

| Entry field | Type | Meaning |
| --- | --- | --- |
| `id` | string | Engine id. Equals the `engine` field of the manifest models it runs, and is passed back as `engineId`. In v0.1: `paddleocr-structure` (Fast), `paddleocr-vl` (Accurate), `deepseek-ocr` (Max). The development build also lists `demo`. |
| `displayName` | string | For logs |
| `capabilities` | object | `tables`, `formulas`, `figures`, `readingOrder`, `boundingBoxes` (booleans) and `languageHandling` (`auto`, `selectable`, `none`) |
| `manifestModels` | string[] | Manifest model ids that must be installed to run this engine |

<!-- example: engine.capabilities.request -->
```json
{"jsonrpc": "2.0", "id": 3, "method": "engine.capabilities"}
```

<!-- example: engine.capabilities.response -->
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": [
    {
      "id": "paddleocr-structure",
      "displayName": "PP-StructureV3 (text and layout)",
      "capabilities": {
        "tables": false,
        "formulas": false,
        "figures": true,
        "readingOrder": true,
        "boundingBoxes": true,
        "languageHandling": "selectable"
      },
      "manifestModels": ["fast-ppocrv6-tiny"]
    },
    {
      "id": "paddleocr-vl",
      "displayName": "PaddleOCR-VL",
      "capabilities": {
        "tables": true,
        "formulas": true,
        "figures": true,
        "readingOrder": true,
        "boundingBoxes": true,
        "languageHandling": "auto"
      },
      "manifestModels": ["accurate-paddleocr-vl-1.6"]
    },
    {
      "id": "deepseek-ocr",
      "displayName": "DeepSeek-OCR",
      "capabilities": {
        "tables": true,
        "formulas": true,
        "figures": true,
        "readingOrder": true,
        "boundingBoxes": true,
        "languageHandling": "auto"
      },
      "manifestModels": ["max-deepseek-ocr-1-q8", "experimental-deepseek-ocr-2-q8"]
    }
  ]
}
```

### engine.configure

Tells the engine what it cannot know. Rust calls it once after startup and again whenever llama-server is started or restarted (new port) or stopped. It may be called any time no other call is in flight, and the engine drops any cached model or connection that depended on the old values.

| Parameter | Type | Required | Meaning |
| --- | --- | --- | --- |
| `llamaServerUrl` | string or null | yes | Base URL such as `http://127.0.0.1:52871`. `null` when no llama-server is running (the Fast tier needs none) |
| `workDir` | path | yes | Scratch directory; the engine keeps its own files under `<workDir>/_engine/` |
| `modelsDir` | path | yes | Root of the installed models (see 1.3) |
| `cpuThreads` | integer | no | CPU threads for the Paddle pipelines. Omitted: the engine decides |

Result: `{ "cpuThreads": <number in effect> }`. Errors: `INVALID_PARAMS`, `BUSY`.

<!-- example: engine.configure.request -->
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "engine.configure",
  "params": {
    "llamaServerUrl": "http://127.0.0.1:52871",
    "workDir": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work",
    "modelsDir": "G:\\OpenDoc Reader\\models",
    "cpuThreads": 8
  }
}
```

<!-- example: engine.configure.response -->
```json
{"jsonrpc": "2.0", "id": 4, "result": {"cpuThreads": 8}}
```

### pdf.inspect

Reads a PDF without rendering it.

| Parameter | Type | Meaning |
| --- | --- | --- |
| `path` | path | The PDF |

| Result field | Meaning |
| --- | --- |
| `pageCount` | Number of pages |
| `pages[].index`, `width`, `height` | 0-based index and size in PDF points (1/72 inch) after the page's own rotation |
| `pages[].hasTextLayer` | The page has a usable text layer: at least 20 non-whitespace characters, mostly printable text rather than replacement or private-use characters |
| `pages[].charCount` | Non-whitespace characters found |

Rust offers "Use existing text" when at least half of the pages have a text layer. Errors: `BAD_INPUT` with `file_not_found`, `unreadable`, `corrupt` or `password_protected`.

<!-- example: pdf.inspect.request -->
```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "pdf.inspect",
  "params": {"path": "C:\\Users\\Ada\\Documents\\invoice.pdf"}
}
```

<!-- example: pdf.inspect.response -->
```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "result": {
    "pageCount": 2,
    "pages": [
      {"index": 0, "width": 595.0, "height": 842.0, "hasTextLayer": true, "charCount": 2310},
      {"index": 1, "width": 595.0, "height": 842.0, "hasTextLayer": false, "charCount": 0}
    ]
  }
}
```

### image.inspect

The same for an image file (PNG, JPG, TIFF, BMP, WebP, HEIC). A multi-page TIFF has one page per frame. Result: `pageCount` and `pages[]` with `index`, `width` and `height` in pixels. Errors as for `pdf.inspect`, except there is no password case.

<!-- example: image.inspect.request -->
```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "image.inspect",
  "params": {"path": "C:\\Users\\Ada\\Pictures\\receipt.tiff"}
}
```

<!-- example: image.inspect.response -->
```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "result": {
    "pageCount": 2,
    "pages": [{"index": 0, "width": 1700, "height": 2200}, {"index": 1, "width": 1700, "height": 2200}]
  }
}
```

### page.render

Produces the page image that every later call refers to. PDF pages are rendered with PDFium at `dpi`. Image pages are decoded (EXIF orientation applied) and written as PNG at their native size. Rotation and deskew then run, in that order. The PNG is written atomically to `outPath`; parent directories are created.

| Parameter | Type | Default | Meaning |
| --- | --- | --- | --- |
| `source` | path | | A PDF or an image file |
| `pageIndex` | integer | | 0-based page (frame) index |
| `outPath` | path | | The PNG to write |
| `dpi` | integer 72 to 600 | 200 | PDF render resolution. Ignored for images. The conversion pipeline always uses 200; other values are for previews |
| `rotate` | `"auto"`, 0, 90, 180 or 270 | `"auto"` | Clockwise degrees to rotate by, or detect the orientation |
| `deskew` | boolean | true | Straighten skew up to about 15 degrees |
| `dewarp` | boolean | false | Flatten curled phone photos |

| Result field | Meaning |
| --- | --- |
| `path` | `outPath` |
| `width`, `height` | Pixels of the written image |
| `dpi` | Resolution used for a PDF page, null for an image |
| `rotation` | Clockwise rotation applied: 0, 90, 180 or 270 |
| `skewAngle` | Degrees of deskew applied, counterclockwise positive, 0 when none |

Orientation detection (`rotate: "auto"`) and dewarping use optional Paddle helper models, `PP-LCNet_x1_0_doc_ori` and `UVDoc`, which the engine looks for in any folder under `modelsDir` (`<modelsDir>/<any>/PP-LCNet_x1_0_doc_ori/`). Without the orientation model, `"auto"` leaves the page as it is and reports rotation 0. Without the dewarp model, `dewarp: true` fails with `MODEL_NOT_READY` (`files_missing`). Deskew is classical and needs no model; it leaves pages without clear text lines alone.

Errors: `BAD_INPUT` (`file_not_found`, `unreadable`, `corrupt`, `password_protected`, `unsupported_format`, `page_out_of_range`), `IO_ERROR`, `OUT_OF_MEMORY`.

<!-- example: page.render.request -->
```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "method": "page.render",
  "params": {
    "source": "C:\\Users\\Ada\\Documents\\invoice.pdf",
    "pageIndex": 3,
    "outPath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\pages\\page-0003.png",
    "dpi": 200,
    "rotate": "auto",
    "deskew": true,
    "dewarp": false
  }
}
```

<!-- example: page.render.response -->
```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "result": {
    "path": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\pages\\page-0003.png",
    "width": 1654,
    "height": 2339,
    "dpi": 200,
    "rotation": 0,
    "skewAngle": -0.4
  }
}
```

### convert.page

Recognizes one page and writes it as an IR `Page`.

| Parameter | Type | Meaning |
| --- | --- | --- |
| `engineId` | string | An id from `engine.capabilities` |
| `modelId` | string | The manifest model to use. Recorded in the IR as `engine.model` |
| `imagePath` | path | A page image written by `page.render` |
| `pageIndex` | integer | Becomes `Page.index` and the prefix of block ids (`p3-b0`, `p3-b1`, ...) |
| `irPath` | path | Where to write the IR `Page` JSON |
| `cropsDir` | path | Where to write figure crops, named `p<pageIndex>-b<n>.png` |
| `options.language` | string, optional | Fast tier only. The VLM tiers detect the language themselves. Not used by the UI in v0.1 |

| Result field | Meaning |
| --- | --- |
| `irPath` | The file written |
| `blockCount` | Blocks on the page |
| `engine` | `{ id, model, version }` as recorded in the IR; Rust passes it on to `document.assemble` |
| `elapsedMs` | Time spent, for speed statistics |
| `warnings` | Notes for the log about degraded output (a malformed box, a table that fell back to text) |

The engine emits `convert.progress` notifications (1.6) while it runs. Errors: `NOT_CONFIGURED`, `MODEL_NOT_READY`, `LLAMA_SERVER_UNREACHABLE`, `OUT_OF_MEMORY`, `RECOGNITION_FAILED`, `BAD_INPUT`, `IO_ERROR`, `CANCELLED`, `BUSY`.

<!-- example: convert.page.request -->
```json
{
  "jsonrpc": "2.0",
  "id": 8,
  "method": "convert.page",
  "params": {
    "engineId": "paddleocr-vl",
    "modelId": "accurate-paddleocr-vl-1.6",
    "imagePath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\pages\\page-0003.png",
    "pageIndex": 3,
    "irPath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\ir\\page-0003.json",
    "cropsDir": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\crops",
    "options": {}
  }
}
```

Notifications sent before the response:

<!-- example: convert.page.notification.1 -->
```json
{
  "jsonrpc": "2.0",
  "method": "convert.progress",
  "params": {"requestId": 8, "pageIndex": 3, "stage": "layout"}
}
```

<!-- example: convert.page.notification.2 -->
```json
{
  "jsonrpc": "2.0",
  "method": "convert.progress",
  "params": {"requestId": 8, "pageIndex": 3, "stage": "recognize", "current": 4, "total": 11}
}
```

<!-- example: convert.page.response -->
```json
{
  "jsonrpc": "2.0",
  "id": 8,
  "result": {
    "irPath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\ir\\page-0003.json",
    "blockCount": 11,
    "engine": {"id": "paddleocr-vl", "model": "accurate-paddleocr-vl-1.6", "version": "0.1.0"},
    "elapsedMs": 4800,
    "warnings": []
  }
}
```

### convert.cancel

| Parameter | Type | Meaning |
| --- | --- | --- |
| `requestId` | integer or string | Id of the call to stop |

Result: `{ "cancelled": true }` when a call with that id was in flight and has been signalled, `{ "cancelled": false }` when there was none (it may have just finished; not an error). The cancelled call then answers with `CANCELLED`, normally within a second or two.

<!-- example: convert.cancel.request -->
```json
{"jsonrpc": "2.0", "id": 9, "method": "convert.cancel", "params": {"requestId": 8}}
```

<!-- example: convert.cancel.response -->
```json
{"jsonrpc": "2.0", "id": 9, "result": {"cancelled": true}}
```

### text.extract

The "Use existing text" path: builds an IR `Page` from a PDF page's text layer, with no recognition. Blocks are headings, paragraphs and lists inferred from font size, position and spacing. There are no tables, formulas or figures, and no `cropsDir`. The result engine is `{ "id": "pdf-text", "model": "text-layer" }`.

`imagePath` must be the image `page.render` wrote for the same page with `rotate: 0` and `deskew: false`, because the engine maps PDF coordinates to image pixels by a plain scale. If the aspect ratio does not match it fails with `BAD_INPUT` and reason `image_mismatch`.

| Parameter | Type | Meaning |
| --- | --- | --- |
| `source` | path | The PDF |
| `pageIndex` | integer | 0-based page |
| `imagePath` | path | The unrotated, undeskewed page image |
| `irPath` | path | Where to write the IR `Page` |

Result: the same as `convert.page`. No progress notifications. Errors: `BAD_INPUT`, `IO_ERROR`, `BUSY`.

<!-- example: text.extract.request -->
```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "method": "text.extract",
  "params": {
    "source": "C:\\Users\\Ada\\Documents\\invoice.pdf",
    "pageIndex": 0,
    "imagePath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\pages\\page-0000.png",
    "irPath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\ir\\page-0000.json"
  }
}
```

<!-- example: text.extract.response -->
```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "result": {
    "irPath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\ir\\page-0000.json",
    "blockCount": 14,
    "engine": {"id": "pdf-text", "model": "text-layer", "version": "0.1.0"},
    "elapsedMs": 90,
    "warnings": []
  }
}
```

### document.assemble

Joins finished IR `Page` files into one `Document` file, so Rust never builds or parses IR itself. It checks that the pages are valid and in order.

| Parameter | Type | Meaning |
| --- | --- | --- |
| `source` | object | `{ path, kind, pageCount }` as in the IR |
| `engine` | object | `{ id, model, version }`, taken from the `convert.page` or `text.extract` results |
| `pages` | path[] | The `Page` files in page order |
| `outPath` | path | The `Document` file to write |

Result: `{ path, pageCount }`. Errors: `BAD_INPUT` (`unreadable` or `corrupt` for a page file that does not parse or validate), `IO_ERROR`.

<!-- example: document.assemble.request -->
```json
{
  "jsonrpc": "2.0",
  "id": 11,
  "method": "document.assemble",
  "params": {
    "source": {"path": "C:\\Users\\Ada\\Documents\\invoice.pdf", "kind": "pdf", "pageCount": 2},
    "engine": {"id": "paddleocr-vl", "model": "accurate-paddleocr-vl-1.6", "version": "0.1.0"},
    "pages": [
      "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\ir\\page-0000.json",
      "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\ir\\page-0001.json"
    ],
    "outPath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\document.json"
  }
}
```

<!-- example: document.assemble.response -->
```json
{
  "jsonrpc": "2.0",
  "id": 11,
  "result": {
    "path": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\document.json",
    "pageCount": 2
  }
}
```

### export.docx

Writes a Word file from the Document IR. Display and inline math becomes native Word equations where the converter supports the construct, and the LaTeX text otherwise (a warning says which). Running heads, footers and page numbers are left out; footnotes stay at the end of their page's section; a page break separates source pages.

| Parameter | Type | Meaning |
| --- | --- | --- |
| `document` | path or object | The path of a `Document` file (normal use), or the Document object itself (tests) |
| `outputPath` | path | The `.docx` to write. Must not exist |

Result: `{ files: [<outputPath>], warnings: [...] }`. Errors: `BAD_INPUT`, `IO_ERROR` (`already_exists`, `disk_full`, `permission_denied`, `write_failed`).

<!-- example: export.docx.request -->
```json
{
  "jsonrpc": "2.0",
  "id": 12,
  "method": "export.docx",
  "params": {
    "document": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\document.json",
    "outputPath": "C:\\Users\\Ada\\Documents\\invoice.docx"
  }
}
```

<!-- example: export.docx.response -->
```json
{
  "jsonrpc": "2.0",
  "id": 12,
  "result": {
    "files": ["C:\\Users\\Ada\\Documents\\invoice.docx"],
    "warnings": ["formula p1-b4 kept as LaTeX text (unsupported construct)"]
  }
}
```

### export.markdown

Writes a Markdown file and copies the figure crops it references into `assetsDir`, which is created only if the document has figures. Links are relative to the `.md` file: `![](<basename of assetsDir>/<file>)`. Page furniture is left out as for Word. Tables are GFM, or HTML when a cell is merged; math is `$...$` and `$$...$$`.

| Parameter | Type | Meaning |
| --- | --- | --- |
| `document` | path or object | As for `export.docx` |
| `outputPath` | path | The `.md` to write. Must not exist |
| `assetsDir` | path | The figure folder, for example `invoice_assets`. No file in it may exist already |

Result: `{ files, warnings }`, where `files` lists the Markdown file and every asset written. Errors as for `export.docx`.

<!-- example: export.markdown.request -->
```json
{
  "jsonrpc": "2.0",
  "id": 13,
  "method": "export.markdown",
  "params": {
    "document": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\document.json",
    "outputPath": "C:\\Users\\Ada\\Documents\\invoice.md",
    "assetsDir": "C:\\Users\\Ada\\Documents\\invoice_assets"
  }
}
```

<!-- example: export.markdown.response -->
```json
{
  "jsonrpc": "2.0",
  "id": 13,
  "result": {
    "files": [
      "C:\\Users\\Ada\\Documents\\invoice.md",
      "C:\\Users\\Ada\\Documents\\invoice_assets\\p1-b2.png"
    ],
    "warnings": []
  }
}
```

### calibrate.run

Times `convert.page` on a sample page so the app can show "About 4.8 s per page on this computer". The first run is a warm-up (model load, first llama-server request) and is reported separately; then `runs` timed runs follow. The engine writes only under its own scratch directory. Progress and cancellation work as for `convert.page`.

| Parameter | Type | Default | Meaning |
| --- | --- | --- | --- |
| `engineId` | string | | As for `convert.page` |
| `modelId` | string | | As for `convert.page` |
| `samplePath` | path | | The bundled sample page image (a PNG, no rendering step) |
| `runs` | integer 1 to 5 | 2 | Timed runs after the warm-up |

| Result field | Meaning |
| --- | --- |
| `secondsPerPage` | Median of the timed runs |
| `warmupSeconds` | The first, untimed-for-the-estimate run |
| `runSeconds` | Each timed run |

Errors: the same as `convert.page`.

<!-- example: calibrate.run.request -->
```json
{
  "jsonrpc": "2.0",
  "id": 14,
  "method": "calibrate.run",
  "params": {
    "engineId": "paddleocr-vl",
    "modelId": "accurate-paddleocr-vl-1.6",
    "samplePath": "C:\\Program Files\\OpenDoc Reader\\resources\\calibration-page.png",
    "runs": 2
  }
}
```

<!-- example: calibrate.run.response -->
```json
{
  "jsonrpc": "2.0",
  "id": 14,
  "result": {"secondsPerPage": 4.8, "warmupSeconds": 9.6, "runSeconds": [4.9, 4.7]}
}
```

## 1.6 Notifications

### convert.progress

Sent by the engine while `convert.page` or `calibrate.run` is running, never after the response to the same request. It has no id. It is informational: Rust uses it for the stage text and as a sign of life.

| Parameter | Type | Meaning |
| --- | --- | --- |
| `requestId` | integer or string | Id of the request it belongs to |
| `pageIndex` | integer | The page being converted (0 for `calibrate.run`) |
| `stage` | string | One of the stage names below |
| `current`, `total` | integer, optional | Progress inside the stage, such as layout regions recognized so far |

Stages, in the order a page passes through them. An engine reports only the stages it has, and may repeat `recognize` with a new `current`.

| Stage | Meaning | Fast | Accurate | Max |
| --- | --- | --- | --- | --- |
| `prepare` | Loading models or connecting to llama-server (usually only on the first page) | yes | yes | yes |
| `layout` | Layout detection | yes | yes | no |
| `recognize` | Text, table and formula recognition | yes | yes, per region | yes, one request |
| `normalize` | Building the IR from the model output | yes | yes | yes |
| `crops` | Writing figure crops | yes | yes | yes |

---

# Part 2. Tauri commands and events

The UI calls the Rust core with Tauri commands (`invoke`) and listens to two events. The UI never talks to the engine or to llama-server.

## 2.1 Conventions

- **Types.** TypeScript types are in [`apps/desktop/src/types/commands.ts`](../apps/desktop/src/types/commands.ts) (hand-written) and [`ir.ts`](../apps/desktop/src/types/ir.ts) (generated from the IR schema). Rust types are in [`apps/desktop/src-tauri/src/contract.rs`](../apps/desktop/src-tauri/src/contract.rs). The examples in this part come from [`protocol-examples.json`](protocol-examples.json); a Rust test and a UI test check that both sides serialize to exactly these shapes.
- **JSON.** camelCase keys, camelCase string values for enums, `null` (never a missing key) for an absent value. Sizes are in bytes, except hardware memory and disk, which are in MB (`ramMB`, `vramMB`, `unifiedMemoryMB`, `freeDiskMB`, the same names the manifest's suggester rules use). Durations are in seconds unless the name ends in `Ms`.
- **Paths** are absolute, in the native OS format. The UI treats them as opaque strings and shows images with Tauri's asset protocol (`convertFileSrc`). The asset scope and the CSP `img-src` must allow the work directory (an M1/M4 task; the current CSP does not).
- **Ids.** `fileId` is assigned by the Rust core when a file is added and is stable across restarts. Model ids are the keys of `models` in `models/manifest.json`.
- **Rust signatures** below omit the `State<'_, AppState>` and `AppHandle` parameters every command may take. Arguments arrive from JavaScript as an object with camelCase names (`invoke("queue_cancel", { fileId })`).
- **Errors.** Every command is `Result<T, AppError>`. A rejected promise carries an `AppError`: `code`, a short English `message` for the log, an optional `reason`, and `detail` for the log. The UI writes its own copy from `code` and `reason` (design.md, section 6). Codes: `invalidArgument`, `notFound`, `badInput`, `modelNotReady`, `outOfMemory`, `llamaServerUnreachable`, `recognitionFailed`, `engineUnavailable`, `cancelled`, `offline`, `downloadFailed`, `checksumMismatch`, `diskFull`, `io`, `internal`.
- **Long-running commands** return as soon as the work has been accepted, with `Result<(), AppError>`. Progress, completion and failure arrive as events, and the matching `*_status` or list command returns the same state, so the UI can recover after a reload. They are: `start_setup_download`, `models_install`, `models_import` (events: `setup://progress`) and `queue_start`, `queue_retry` (events: `queue://progress`). All other commands resolve with their result when done. `export` is the slowest of those (a few seconds) and has no events.

An `AppError`:

<!-- example: tauri.AppError -->
```json
{
  "code": "badInput",
  "message": "The file cannot be converted.",
  "reason": "password_protected",
  "detail": null
}
```

## 2.2 Commands

### Setup, hardware and models

| Command | Rust signature | Resolves with | Notes |
| --- | --- | --- | --- |
| `probe_hardware` | `async fn probe_hardware() -> Result<HardwareInfo, AppError>` | `HardwareInfo` | Probes now (GPU, memory, CPU, free disk on the models drive) and caches the result. A few hundred milliseconds |
| `get_recommendation` | `async fn get_recommendation() -> Result<Recommendation, AppError>` | `Recommendation` | Applies the manifest rules to the last probe (probing first if there is none). One pick with a reason, a fit badge and a speed estimate per model |
| `start_setup_download` | `async fn start_setup_download(model_ids: Vec<String>) -> Result<(), AppError>` | nothing, **long-running** | Downloads the runtime for this machine, creates the engine environment, downloads the models, then runs the calibration. Resumable: calling it again after a failure or restart continues. Events: `setup://progress` |
| `setup_pause` | `async fn setup_pause() -> Result<(), AppError>` | nothing | Pauses all downloads (keeps partial files). `phase` becomes `paused` |
| `setup_resume` | `async fn setup_resume() -> Result<(), AppError>` | nothing | |
| `setup_status` | `async fn setup_status() -> Result<SetupStatus, AppError>` | `SetupStatus` | The same snapshot the event carries. Call it on startup to decide whether to show Setup (`phase` is not `ready`) |
| `models_list` | `async fn models_list() -> Result<Vec<ModelInfo>, AppError>` | `ModelInfo[]` | Every model in the manifest, installed or not, experimental ones included |
| `models_install` | `async fn models_install(model_id: String) -> Result<(), AppError>` | nothing, **long-running** | From Settings. Same download manager and same event as setup. A model that does not fit this machine is rejected with `invalidArgument` |
| `models_remove` | `async fn models_remove(model_id: String) -> Result<(), AppError>` | nothing | Deletes the model's files. Rejected with `invalidArgument` while a conversion uses it |
| `models_import` | `async fn models_import(model_id: String, folder: String) -> Result<(), AppError>` | nothing, **long-running** | "Import model from folder": checks every file against the manifest SHA-256 and copies it into the models folder. A mismatch rejects with `checksumMismatch` or reports it through `setup://progress` |

### Queue

| Command | Rust signature | Resolves with | Notes |
| --- | --- | --- | --- |
| `queue_add` | `async fn queue_add(paths: Vec<String>) -> Result<Vec<QueueItem>, AppError>` | the items added | Folders are searched recursively. Files of unsupported types are skipped; if nothing is added the call rejects with `badInput` and reason `unsupported_format`. A path already waiting or converting is not added twice. New items are `waiting`; Rust inspects them in the background, which fills `pageCount`, `hasTextLayer` and `previewPath` and may move a PDF with text to `needsAttention` |
| `queue_list` | `async fn queue_list() -> Result<Vec<QueueItem>, AppError>` | `QueueItem[]` in queue order | Items carry no per-page detail (see `get_pages`) |
| `queue_start` | `async fn queue_start(options: ConvertOptions) -> Result<(), AppError>` | nothing, **long-running** | Stamps `modelId` and `formats` on every `waiting` item and runs them in order, one at a time. Items that are `needsAttention` wait until resolved. Rejected with `modelNotReady` if the model is not installed. Events: `queue://progress` |
| `queue_pause` | `async fn queue_pause() -> Result<(), AppError>` | nothing | Stops after the page in flight completes. The converting item becomes `paused` |
| `queue_resume` | `async fn queue_resume() -> Result<(), AppError>` | nothing | |
| `queue_cancel` | `async fn queue_cancel(file_id: Option<String>) -> Result<(), AppError>` | nothing | One item, or every waiting, converting and paused item when `None`. Cancels the page in flight (`convert.cancel`) and keeps the pages already finished, so a later `queue_retry` resumes |
| `queue_remove` | `async fn queue_remove(file_id: String) -> Result<(), AppError>` | nothing | Removes an item that is not converting and deletes its job directory. Outputs already written stay |
| `queue_clear_done` | `async fn queue_clear_done() -> Result<Vec<String>, AppError>` | the removed `fileId`s | Removes every `done` and `cancelled` item ("Clear done") |
| `queue_retry` | `async fn queue_retry(file_id: String) -> Result<(), AppError>` | nothing, **long-running** | For a `failed` or `cancelled` item: queues it again with its previous options and continues from the first page that has no result. Also the "Add again" action |
| `queue_set_text_mode` | `async fn queue_set_text_mode(file_id: String, mode: TextMode) -> Result<(), AppError>` | nothing | Answers the "This PDF already contains text" notice: `existingText` ("Use existing text", runs `text.extract`) or `ocr` ("Run OCR anyway"). The item moves from `needsAttention` to `waiting` |
| `get_pages` | `async fn get_pages(file_id: String) -> Result<Vec<PageStatus>, AppError>` | `PageStatus[]` | One entry per page with its state and rendered image, for the thumbnail strip |
| `get_review` | `async fn get_review(file_id: String, page: u32) -> Result<ReviewPage, AppError>` | `ReviewPage` | The IR page (read from the job's `ir/` file and passed through unchanged), its image path and the engine that produced it. `page` is 0-based. Rejects with `notFound` for a page that has no result yet |
| `export` | `async fn export(file_id: String, format: ExportFormat) -> Result<OutputFile, AppError>` | `OutputFile` | Writes one more export of a finished file ("Export Word", "Export Markdown"): assembles the document if needed, picks a free name next to the original (or in the chosen folder) and runs `export.docx` or `export.markdown`. Never overwrites, so a second call writes "name (2).docx" |
| `reveal_in_folder` | `async fn reveal_in_folder(path: String) -> Result<(), AppError>` | nothing | Shows the file in the OS file manager |

### Settings

| Command | Rust signature | Resolves with | Notes |
| --- | --- | --- | --- |
| `settings_get` | `async fn settings_get() -> Result<Settings, AppError>` | `Settings` | |
| `settings_set` | `async fn settings_set(patch: SettingsPatch) -> Result<Settings, AppError>` | the full `Settings` after the change | A field that is missing or `null` in the patch is left alone. Settings apply immediately. Changing `modelsDir` is only accepted while no model is installed or downloading, otherwise `invalidArgument` |

### Misc

| Command | Rust signature | Resolves with | Notes |
| --- | --- | --- | --- |
| `app_version` | `fn app_version() -> String` | the version string | Exists already |
| `get_diagnostics` | `async fn get_diagnostics() -> Result<String, AppError>` | text | The "Copy diagnostics" text: versions, OS, hardware, installed models, recent log lines. No file names or document content |
| `open_logs_folder` | `async fn open_logs_folder() -> Result<(), AppError>` | nothing | |

## 2.3 Types

The full definitions are in `commands.ts`. This section shows the shape of the main ones with examples.

**`HardwareInfo`** is what Setup step 2 shows.

<!-- example: tauri.HardwareInfo -->
```json
{
  "os": "windows",
  "gpus": [{"vendor": "nvidia", "name": "NVIDIA GeForce RTX 3060", "vramMB": 12288}],
  "backend": "cuda",
  "ramMB": 32768,
  "unifiedMemoryMB": null,
  "cpuName": "AMD Ryzen 7 5800X 8-Core Processor",
  "cpuCores": 16,
  "modelsDir": "G:\\OpenDoc Reader\\models",
  "freeDiskMB": 412000,
  "cudaDriverVersion": "576.80"
}
```

**`Recommendation`** is what Setup step 3 shows: the suggester's pick with its reason (placeholders from the manifest already filled in), a fit badge per model (`runsWell`, `slow`, `wontFit`, `hidden`) and a speed estimate with its basis (`measured`, `estimated` or `calibrated`). `runtimeDownloadBytes` is the llama-server build plus the engine environment, so the UI can show "Model 1.8 GB, runtime and engine 0.8 GB".

<!-- example: tauri.Recommendation -->
```json
{
  "modelId": "max-deepseek-ocr-1-q8",
  "reason": "Your NVIDIA GeForce RTX 3060 (12 GB) runs Max on the GPU.",
  "fits": [
    {
      "modelId": "fast-ppocrv6-tiny",
      "badge": "runsWell",
      "speed": {"secondsPerPage": 8.4, "basis": "estimated"}
    },
    {
      "modelId": "accurate-paddleocr-vl-1.6",
      "badge": "runsWell",
      "speed": {"secondsPerPage": 4.8, "basis": "measured"}
    },
    {
      "modelId": "max-deepseek-ocr-1-q8",
      "badge": "runsWell",
      "speed": {"secondsPerPage": 5.2, "basis": "measured"}
    },
    {"modelId": "experimental-deepseek-ocr-2-q8", "badge": "hidden", "speed": null}
  ],
  "runtimeDownloadBytes": 1500000000
}
```

**`ModelInfo`** is a row in Settings and the "Model details" popover.

<!-- example: tauri.ModelInfo -->
```json
{
  "id": "accurate-paddleocr-vl-1.6",
  "tier": "accurate",
  "displayName": "PaddleOCR-VL 1.6",
  "engineId": "paddleocr-vl",
  "experimental": false,
  "license": "Apache-2.0",
  "downloadSizeBytes": 1949544560,
  "installedSizeBytes": null,
  "state": "notInstalled",
  "sourceUrl": "https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6-GGUF"
}
```

**`SetupStatus`** is both the result of `setup_status` and the payload of `setup://progress`. `phase` is `notStarted`, `downloading`, `paused`, `calibrating`, `ready` or `failed`. `components` lists the llama-server runtime, the document engine environment and each model, with bytes, speed and time left; a `bytesTotal` of 0 means the total is unknown and the bar is indeterminate. `calibration` holds the measured seconds per page after the calibration run.

<!-- example: tauri.SetupStatus -->
```json
{
  "phase": "downloading",
  "modelsDir": "G:\\OpenDoc Reader\\models",
  "components": [
    {
      "id": "llama-server",
      "kind": "runtime",
      "name": "llama-server (CUDA)",
      "state": "done",
      "bytesDone": 578117583,
      "bytesTotal": 578117583,
      "bytesPerSecond": null,
      "secondsLeft": null
    },
    {
      "id": "engine",
      "kind": "engine",
      "name": "Document engine",
      "state": "downloading",
      "bytesDone": 0,
      "bytesTotal": 0,
      "bytesPerSecond": null,
      "secondsLeft": null
    },
    {
      "id": "accurate-paddleocr-vl-1.6",
      "kind": "model",
      "name": "PaddleOCR-VL 1.6",
      "state": "downloading",
      "bytesDone": 912000000,
      "bytesTotal": 1949544560,
      "bytesPerSecond": 18500000.5,
      "secondsLeft": 56.4
    }
  ],
  "calibration": null,
  "error": null
}
```

**`Settings`** and **`SettingsPatch`**. `saveTo` is `{ "kind": "original" }` or `{ "kind": "folder", "path": ... }`. `defaultModelId` is null when the app should use the recommendation.

<!-- example: tauri.Settings -->
```json
{
  "defaultFormats": ["docx", "markdown"],
  "defaultModelId": null,
  "saveTo": {"kind": "original"},
  "theme": "system",
  "checkForUpdates": true,
  "modelsDir": "G:\\OpenDoc Reader\\models",
  "experimentalVulkan": false
}
```

<!-- example: tauri.SettingsPatch -->
```json
{"theme": "dark", "saveTo": {"kind": "folder", "path": "D:\\Converted"}}
```

**`ConvertOptions`** is the argument of `queue_start`: the action bar's choices.

<!-- example: tauri.ConvertOptions -->
```json
{
  "formats": ["docx", "markdown"],
  "modelId": "accurate-paddleocr-vl-1.6",
  "saveTo": {"kind": "original"}
}
```

**`QueueItem`** is one row of the queue and everything the detail view needs. `status` is `waiting`, `converting`, `paused`, `needsAttention`, `done`, `failed` or `cancelled`. `attention` is set with `needsAttention` (only `hasTextLayer` in v0.1). `stage` is one of `render`, `prepare`, `layout`, `recognize`, `normalize`, `crops`, `export`; the first and last run in the Rust core, the rest are the engine's stages. `textMode` is null until the item is started (or the user decides). `outputs` fills when the exports are written; `durationMs` is the time of the whole conversion, for "Finished in 14 s".

<!-- example: tauri.QueueItem -->
```json
{
  "fileId": "f-9c41",
  "path": "C:\\Users\\Ada\\Documents\\invoice.pdf",
  "name": "invoice.pdf",
  "kind": "pdf",
  "sizeBytes": 1843200,
  "pageCount": 12,
  "hasTextLayer": false,
  "previewPath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\pages\\page-0000.png",
  "status": "converting",
  "attention": null,
  "textMode": "ocr",
  "modelId": "accurate-paddleocr-vl-1.6",
  "formats": ["docx", "markdown"],
  "pagesDone": 3,
  "currentPage": 3,
  "stage": "recognize",
  "secondsLeft": 43.5,
  "durationMs": null,
  "outputs": [],
  "error": null
}
```

**`PageStatus`** has `state` of `pending`, `converting`, `done` or `failed`.

<!-- example: tauri.PageStatus -->
```json
{
  "index": 3,
  "state": "converting",
  "imagePath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\pages\\page-0003.png",
  "error": null
}
```

**`OutputFile`** is a written export. `assetsDir` is set for Markdown when figures were written.

<!-- example: tauri.OutputFile -->
```json
{
  "format": "markdown",
  "path": "C:\\Users\\Ada\\Documents\\invoice.md",
  "assetsDir": "C:\\Users\\Ada\\Documents\\invoice_assets"
}
```

**`ReviewPage`** carries the IR page (type `Page` from `ir.ts`). The UI draws block overlays from `page.blocks[].bbox`, in the pixel space of `imagePath`, and renders tables, formulas and figures from the same blocks.

<!-- example: tauri.ReviewPage -->
```json
{
  "fileId": "f-9c41",
  "pageIndex": 3,
  "pageCount": 12,
  "imagePath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\pages\\page-0003.png",
  "page": {
    "index": 3,
    "width": 1654,
    "height": 2339,
    "sourceImage": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\pages\\page-0003.png",
    "blocks": [
      {
        "id": "p3-b0",
        "type": "heading",
        "bbox": [165.4, 140.2, 1488.6, 233.9],
        "order": 0,
        "level": 1,
        "text": "3. Results"
      },
      {
        "id": "p3-b1",
        "type": "paragraph",
        "bbox": [165.4, 280.0, 1488.6, 520.0],
        "order": 1,
        "text": "The energy of a body at rest is given by"
      },
      {
        "id": "p3-b2",
        "type": "formula",
        "bbox": [496.2, 560.0, 1157.8, 640.0],
        "order": 2,
        "latex": "E = mc^2",
        "number": "(1)"
      }
    ]
  },
  "engine": {"id": "paddleocr-vl", "model": "accurate-paddleocr-vl-1.6", "version": "0.1.0"}
}
```

## 2.4 Events

Both events are emitted to the main window. Subscribe with `listen`, and call `queue_list` or `setup_status` once at startup (and after a reconnect) to get the starting state. Payloads are full snapshots, so a missed event is repaired by the next one. Rust coalesces bursts: at most about 10 events per second per file for the queue, and about 4 per second for setup, with phase changes sent at once.

| Event | Payload | When |
| --- | --- | --- |
| `queue://progress` | `QueueProgress` = `{ item: QueueItem, page: PageStatus or null }` | Whenever an item changes (added, started, stage change, page finished, paused, done, failed, cancelled). `item` is the item's complete current state: replace the item with the same `fileId`. `page` is the page whose state just changed, or null for item-level changes. Items removed by `queue_remove` or `queue_clear_done` are not announced; the UI already knows, because it asked |
| `setup://progress` | `SetupStatus` | Whenever a download progresses or the phase changes, from `start_setup_download`, `models_install` and `models_import` alike |

`queue://progress`:

<!-- example: tauri.event.queue-progress -->
```json
{
  "item": {
    "fileId": "f-9c41",
    "path": "C:\\Users\\Ada\\Documents\\invoice.pdf",
    "name": "invoice.pdf",
    "kind": "pdf",
    "sizeBytes": 1843200,
    "pageCount": 12,
    "hasTextLayer": false,
    "previewPath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\pages\\page-0000.png",
    "status": "converting",
    "attention": null,
    "textMode": "ocr",
    "modelId": "accurate-paddleocr-vl-1.6",
    "formats": ["docx", "markdown"],
    "pagesDone": 4,
    "currentPage": 4,
    "stage": "render",
    "secondsLeft": 38.7,
    "durationMs": null,
    "outputs": [],
    "error": null
  },
  "page": {
    "index": 3,
    "state": "done",
    "imagePath": "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41\\pages\\page-0003.png",
    "error": null
  }
}
```

`setup://progress`:

<!-- example: tauri.event.setup-progress -->
```json
{
  "phase": "calibrating",
  "modelsDir": "G:\\OpenDoc Reader\\models",
  "components": [
    {
      "id": "accurate-paddleocr-vl-1.6",
      "kind": "model",
      "name": "PaddleOCR-VL 1.6",
      "state": "done",
      "bytesDone": 1949544560,
      "bytesTotal": 1949544560,
      "bytesPerSecond": null,
      "secondsLeft": null
    }
  ],
  "calibration": null,
  "error": null
}
```

## 2.5 How engine errors reach the UI

Rust maps engine errors to `AppError` codes like this. It keeps the engine's `reason` and puts the engine's message and `data.detail` into `detail`.

| Engine error | `AppError.code` | What Rust also does |
| --- | --- | --- |
| `MODEL_NOT_READY` | `modelNotReady` | Fails the page; the UI suggests reinstalling the model |
| `LLAMA_SERVER_UNREACHABLE` | `llamaServerUnreachable` | Restarts llama-server and retries the page once |
| `OUT_OF_MEMORY` | `outOfMemory` | Fails the item; the UI suggests a smaller model |
| `BAD_INPUT` | `badInput` | Fails the item (or the page, for `page_out_of_range`) |
| `RECOGNITION_FAILED` | `recognitionFailed` | Fails the page; retry is offered |
| `CANCELLED` | `cancelled` | Marks the item `cancelled` |
| `IO_ERROR` with `disk_full` | `diskFull` | |
| `IO_ERROR`, other reasons | `io` | |
| `NOT_CONFIGURED` | `internal` | A bug in Rust: it calls `engine.configure` and retries once |
| `BUSY`, `INVALID_PARAMS`, `INVALID_REQUEST`, `METHOD_NOT_FOUND`, `PARSE_ERROR`, `INTERNAL_ERROR` | `internal` | A contract bug; logged with the request |
| Engine exited, stdout closed, protocol version mismatch | `engineUnavailable` | Restarts the engine once, then fails the item |

Download failures use `offline`, `downloadFailed`, `checksumMismatch` and `diskFull` directly; they never involve the engine.

## 2.6 Typical sequences

**First run.** `probe_hardware`, then `get_recommendation`. The user picks models and a folder (`settings_set` with `modelsDir`), then `start_setup_download`. The UI renders `setup://progress` until `phase` is `ready`. Rust calibrates the default model with `calibrate.run` after the downloads, and `SetupStatus.calibration` fills in.

**Converting a PDF.** `queue_add` returns the item as `waiting`. Rust inspects it (`pdf.inspect`, and `page.render` for the preview) and emits `queue://progress`. If the PDF has text, the item becomes `needsAttention` until `queue_set_text_mode`. `queue_start` stamps the options on the waiting items. For each item Rust loops over the pages: `page.render`, then `convert.page` (or `text.extract` for `existingText`), emitting a `queue://progress` after each page. After the last page it calls `document.assemble` and then `export.docx` and `export.markdown` for the chosen formats into free file names, and the item becomes `done`.

**Cancel and resume.** `queue_cancel` sends `convert.cancel`, the page in flight is dropped, and the item becomes `cancelled`. `queue_retry` starts it again and skips every page that has an IR file.

**Review.** `get_pages` for the thumbnail strip, `get_review` for the page on screen, `export` for the toolbar buttons, `reveal_in_folder` for "Show in folder".

---

# Part 3. Keeping the contracts honest

| What | Where | Checked by |
| --- | --- | --- |
| IR schema | `engine/schema/ir.schema.json` | `engine/tests/test_ir_schema.py`: output of the demo engine and the grounding parser validates, the IR round-trips exactly |
| RPC message schemas | `engine/schema/rpc/*.schema.json` | `engine/tests/test_rpc_schema.py`: every example in this document validates, the error table matches `rpc.py`, the live `engine.*` methods answer in the schema's shape |
| Examples | `docs/protocol-examples.json` | The tests above, `src-tauri/src/contract.rs` (Rust round trip) and `apps/desktop/src/types/contract.test.ts` |
| This document | `docs/protocol.md` | `test_rpc_schema.py` checks that every example block here equals its entry in the examples file |
| Generated TypeScript types | `apps/desktop/src/types/ir.ts` | `pnpm --filter desktop lint` fails when the file differs from what `pnpm --filter desktop gen:types` produces |

An example block is the line `<!-- example: key -->` followed by a fenced `json` block. To change an example, edit the JSON file and the block here together.

Changing a contract: edit the schema or the types, the examples and this document in one pull request. A change that breaks an existing reader (a renamed or removed field, a changed meaning) bumps `schemaVersion` (IR) or the protocol version (`engine.version`), and Rust refuses to run with an engine whose versions it does not know. Adding an optional field is not breaking.
