# Model spike report, October 2026

Date: 2026-10-15. Status: findings are measured; decisions in section 8 were accepted on 2026-10-15.

This spike answers the open questions at the end of [architecture.md](../architecture.md). It ran on one machine with five sample pages, so treat quality findings as indications, not scores.

## 1. Summary

- Tested DeepSeek-OCR v1 (Q8_0), DeepSeek-OCR 2 (Q8_0 and Q4_K_M), PaddleOCR-VL 1.6 and the Fast-tier PaddleOCR pipelines, on five pages: a book scan, a table scan, a math scan, a two-column paper and a form.
- On an RTX 4090, DeepSeek v1 averaged 2.7 s/page and PaddleOCR-VL 6.7 s/page (median of 3 warm runs, one machine). On CPU with 16 threads, DeepSeek v1 took 17 to 41 s/page and PaddleOCR-VL 20 to 109 s/page.
- Full GPU offload needs roughly 5.7 GB of VRAM for DeepSeek v1 Q8, 7.3 GB for DeepSeek 2 Q8 and 2.3 GB for PaddleOCR-VL. An 8 GB card was not tested.
- Quality (5 pages, one run each, no engine error-free): DeepSeek v1 was best on the math page and made the most cell errors on the table. PaddleOCR-VL was cleanest on the two-column paper but dropped equation numbers. DeepSeek 2 showed no advantage over v1.
- The Fast tier is much slower in its default configuration than the architecture doc assumed. A stripped-down configuration reaches 4 to 11 s/page, but tables and formulas come out as images.
- Accepted: ship DeepSeek-OCR v1 in Max, offer DeepSeek-OCR 2 as an experimental advanced option, recommend Accurate by default, redefine Fast, and stop assuming Accurate is faster than Max on CPU (section 8).

## 2. Setup

Machine: Windows 11 Pro, RTX 4090 24 GB (driver 616.56, CUDA 13.4), Ryzen 7 7800X3D (8C/16T), 64 GB RAM. Desktop VRAM baseline was 2.5 to 2.95 GB; all VRAM figures below are above that baseline.

**llama.cpp build b11669** (CUDA 13.4 and multi-variant CPU builds, `llama-server` reports `0.7.0-dev`). Pin the manifest to the `bNNNNN` tag. The GitHub "Latest" release is `v0.7.0`, which has no binaries; its only asset is `nightly-tag.txt` containing `b11669`. The server's default port also changed to 9931, so always pass `--port`.

| Model | Repo @ revision | Quant | Size |
| --- | --- | --- | --- |
| DeepSeek-OCR v1 | ggml-org/DeepSeek-OCR-GGUF @ 7086620c | Q8_0 (+ Q8_0 mmproj) | 3.13 GB + 0.45 GB |
| DeepSeek-OCR 2 | sabafallah/DeepSeek-OCR-2-GGUF @ d08e5af4 | Q8_0, Q4_K_M (+ Q8_0 mmproj) | 3.13 GB, 1.95 GB + 0.51 GB |
| PaddleOCR-VL 1.6 | PaddlePaddle/PaddleOCR-VL-1.6-GGUF @ 511b0964 | single published file (precision not checked) | 0.94 GB + 0.88 GB |

DeepSeek-OCR 2 has no official GGUF. The sabafallah repo was the most downloaded community conversion; its provenance is an individual account with no license field. Its files were made for an unmerged-at-the-time PR but load on b11669 unchanged. There is no Q4 mmproj, so the Q4_K_M runs used the Q8_0 mmproj.

<details>
<summary>SHA-256 of the files used</summary>

| File | SHA-256 |
| --- | --- |
| DeepSeek-OCR-Q8_0.gguf | 81ede3e256230707dccf7fa052570c3a939d57db99de655f43cbb1a830d14d92 |
| mmproj-DeepSeek-OCR-Q8_0.gguf | 786c9b5159898de3d1d94a102836df559fed0bcf09f41a32f62c3219b0e278e0 |
| deepseek-ocr-2-q8_0.gguf | 4d82d12b949a6256a66eed9874a8177a9a25d1fe1b0650dfaaf34c9b92d9b911 |
| deepseek-ocr-2-Q4_K_M.gguf | 2b5625a1d649670fa4300a1ca139869850a86e9f3ad5b8e389e2133a210b139d |
| mmproj-deepseek-ocr-2-q8_0.gguf | 46efe1304f581869f3b067eb77b1151dc0daf32bc25ef644fc7d505f4153a545 |
| PaddleOCR-VL-1.6-GGUF.gguf | f3ae46ec885050acf4b3d31944431e1fd90d50664fb09126af4a3c050ba14ee8 |
| PaddleOCR-VL-1.6-GGUF-mmproj.gguf | 204d757d7610d9b3faab10d506d69e5b244e32bf765e2bab2d0167e65e0a058a |
| llama-b11669-bin-win-cuda-13.4-x64.zip | 3b92d5c064f5032e603e531d3ec8961f3bd6613e5dfad047399a92dec8e95dbc |
| cudart-llama-bin-win-cuda-13.4-x64.zip | 738f8c251ac22b70c3ae6f83a10cf222725df0395246a2cf58f32bdb85fbe668 |
| llama-b11669-bin-win-cpu-x64.zip | 37b3efcf962847ca869745fd4ccf7f5e7e8dacfeff9fa3523b4ca8cdc66d632b |

</details>

**Python stack** (CPython 3.12.5, CPU only, no torch): paddlepaddle 3.3.1, paddleocr 3.7.0, paddlex 3.7.2, pypdfium2 5.14.0, opencv-contrib-python 4.10.0.84, numpy 2.3.5.

**Samples.** The page images are for local use and are not committed.

| Short name | Page | Source | License |
| --- | --- | --- | --- |
| book | Tom Sawyer (1876), p. 91, real scan | Internet Archive `adventuresoftoms00twaiiala` | Public domain |
| table | Statistical Abstract 1920, p. 95, real scan | Internet Archive `statisticalabstr1920unse` | Public domain (US federal) |
| math | Granville and Smith, Calculus (1911), p. 77, real scan | Internet Archive `elementsofdiffer00granuoft` | Public domain |
| twocol | arXiv:2610.20039, PDF p. 2, clean render | arxiv.org | CC BY 4.0 |
| form | IRS Form W-9, p. 1, clean render | irs.gov | Public domain (US federal) |

Only three pages are real scans (noise, skew, paper tone). PDF pages were rendered at 200 DPI.

## 3. Speed and memory

Method: one llama-server per config, `-np 1`, temperature 0, prompt cache off. Cold = first request after server start (not in the medians). Then 3 warm rounds over the five pages, median per page. Outputs were byte-identical across warm runs. DeepSeek 2 rows use DRY sampling. For PaddleOCR-VL, the time includes about 2.8 s per page for PP-DocLayoutV3 on CPU.

Median seconds per page, with peak VRAM above baseline and peak RAM (working set of the server, including the mmapped model; PaddleOCR-VL adds the Python process, about 0.9 GB):

| Config | book | table | math | twocol | form | VRAM (MiB) | RAM (MiB) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DS-OCR v1 Q8, GPU | 1.6 | 3.9 | 2.2 | 2.1 | 3.9 | 6012 | 4279 |
| DS-OCR v1 Q8, CPU-16 | 16.9 | 40.1 | 23.6 | 23.4 | 40.8 | 0 | 6201 |
| DS-OCR v1 Q8, CPU-4 | 21.0 | 41.8 | 26.8 | 26.7 | 42.7 | 0 | 6201 |
| DS-OCR 2 Q8, GPU | 1.8 | 4.6 | 2.8 | 2.5 | 4.2 | 7639 | 4381 |
| DS-OCR 2 Q8, CPU-16 | 23.0 | 47.7 | 33.7 | 31.0 | 47.9 | 0 | 7931 |
| DS-OCR 2 Q8, CPU-4 | 29.6 | 51.4 | 39.1 | 36.5 | 51.4 | 0 | 7930 |
| DS-OCR 2 Q4_K_M, GPU | 1.7 | 4.1 | 2.8 | 2.3 | 4.2 | 6599 | 3374 |
| DS-OCR 2 Q4_K_M, CPU-16 | 21.1 | 40.5 | 28.4 | 27.2 | 39.8 | 0 | 7829 |
| DS-OCR 2 Q4_K_M, CPU-4 | 27.3 | 43.7 | 33.4 | 32.4 | 42.9 | 0 | 7827 |
| PaddleOCR-VL 1.6, GPU | 4.4 | 7.0 | 5.4 | 5.2 | 11.4 | 2509 | 1486 + 947 |
| PaddleOCR-VL 1.6, CPU-16 | 19.8 | 51.5 | 38.0 | 39.0 | 108.9 | 0 | 2191 + 926 |
| PaddleOCR-VL 1.6, CPU-4 | 30.4 | 66.6 | 60.4 | 63.0 | 139.3 | 0 | 2191 + 898 |

Notes:

- 4 threads is not a laptop proxy on this CPU. Decode is memory-bandwidth bound and was no slower at 4 threads than at 16 (v1: about 70 vs 63 tok/s); only prefill slowed down. The 7800X3D has a large cache, high clocks and fast memory, so real laptops will be slower than any row above.
- Both DeepSeek models tile pages by default in b11669 (v1: 1x3 grid plus a global view, 914 prompt tokens; v2: 2x3 grid plus a global view, 1132 tokens). All five pages get the same grid. An earlier note in the setup log said only the global view was used; that was wrong.
- Q4_K_M decodes faster than Q8_0 on CPU but its working set is not smaller (7.8 vs 7.9 GB), because the vision encoder buffers dominate.

Memory floors, GPU, `-np 1`, flash-attn off, VRAM above baseline:

| Model | -c | Peak (MiB) | Note |
| --- | --- | --- | --- |
| DS-OCR v1 Q8_0 | 8192 / 4096 | 6012 / 5676 | |
| DS-OCR 2 Q8_0 | 8192 / 4096 | 7639 / 7306 | |
| DS-OCR 2 Q4_K_M | 8192 / 4096 | 6599 / 6271 | |
| DS-OCR 2 Q4_K_M, `--no-mmproj-offload` | 4096 | 2563 | 16 to 18 s/page instead of 1.5 to 4.3 s |
| PaddleOCR-VL 1.6 | 16384 / 8192 | 2509 / 2346 | |

Weights are only 2.0 to 2.8 GB of the DeepSeek figures. Most of the rest is the vision-encoder compute buffer, which grows with tile count. A smaller context saves only about 0.3 GB.

PaddleOCR-VL server flag note: with no `-np` or `-c`, llama-server used 4 slots and the model's 131072-token context, which took 11.5 GB of VRAM. With `-np 1 -c 16384` it used 2.5 GB, at the same speed.

### Apple Silicon estimates

Not measured. No Apple Silicon machine was benchmarked; these are estimates derived from memory bandwidth and the RTX 4090 and CPU measurements above, to be replaced by real numbers when available.

| Machine | Accurate (PaddleOCR-VL) | Max (DeepSeek-OCR v1 Q8) | Notes |
| --- | --- | --- | --- |
| M1/M2/M3 base, 8 GB | 10-30 s/page | not recommended | DS v1 needs about 6 GB on top of macOS; too tight |
| M1/M2/M3/M4 base, 16 GB | 8-25 s/page | 10-40 s/page | |
| M-series Pro/Max, 18 GB+ | 5-15 s/page | 4-15 s/page | higher memory bandwidth |

Basis: decode is memory-bandwidth bound (base M-series about 68-120 GB/s, Pro about 150-270, Max about 300-550, versus about 1 TB/s on the 4090). Dense pages generate up to about 1,900 tokens. PP-DocLayoutV3 on CPU adds about 2-3 s/page.

## 4. Quality review

Small sample: 5 pages, one run each. No engine is error-free.

**Table page** (real scan, about 43 rows by 10 columns, about 400 cells; errors checked against zoomed crops):

- DS-OCR v1 Q8: 5 wrong numbers (for example 2,374 for 2,274; 1,499 for 1,469; 193 for 198; 40 for 49; 123 for 128) and one misaligned row with blank or ditto cells (Dalmatian). Header structure (rowspan/colspan) was correct.
- DS-OCR 2 Q8: 2 to 3 wrong numbers, but the second header row was garbled ("46 to 44", "Over.", "Under 16. 44.", an empty cell) and two rows with blank cells were shifted (Dalmatian, Pacific Islander). Q4_K_M also read the year header "1919" as "2019".
- PaddleOCR-VL 1.6: 4 wrong numbers (49 for 40; 8,903 for 8,003; 120 for 130; 40 for 49) and two shifted rows with blank cells. Header structure was correct.
- All engines struggle with rows that use dotted leaders as blank cells.

**Math page** (real scan): DS-OCR v1 was best overall, with clean LaTeX and equation numbers (2) to (6) kept. PaddleOCR-VL had the cleanest LaTeX (aligned environments) but dropped equation numbers (2) to (6) from the markdown. DS-OCR 2 was noticeably worse: missing subscripts (dy for dy_1), a spurious prime, "length of subtangent" rendered as spaced `\operatorname` letters, and TP_1 rendered as TP in text. Footnotes appeared in the PaddleOCR-VL raw output and the DS-OCR 2 Q4 output, and were missing from DS-OCR v1 and DS-OCR 2 Q8.

**Two-column paper** (clean render): all three VLMs read the columns in the right order. DS-OCR v1 and v2 misread lambda_1 as lambda_l, and v1 inserts spaces after hyphens ("energy- efficient"). PaddleOCR-VL had no errors in the reviewed section.

**Book page** (real scan): the VLMs were within a handful of words of each other, mostly straight versus curly quotes. DS-OCR 2 changed the dialect spelling "weepon" to "weapon". It rewrites what it takes for typos, which is a fidelity risk. The Fast tier (PP-OCRv6 tiny) garbled the running head ("INFUNFOE ExPLAINS") and had minor spacing errors.

**Other findings**

- DS-OCR 2 Q4_K_M loops on the W-9 form without DRY sampling (deterministic across 3 runs, runs to max_tokens). Q8_0 did not loop. DRY changes the output on every page, including the clean ones, and costs about 12% decode speed.
- PaddleOCR-VL returns structured JSON with a layout label and box per block, which maps directly onto the planned IR. DeepSeek grounding gives labels and boxes too, but they must be parsed from the raw token stream.
- Overall: DS-OCR 2 showed no quality advantage over v1 on these pages, needs a sampling workaround, has a higher VRAM floor (7.3 vs 5.7 GB) and no official GGUF. v1 has an official ggml-org GGUF and an MIT license.

## 5. Fast tier investigation

The architecture doc estimated seconds per page. The full PP-StructureV3 default (PP-OCRv5 server models) took 61, 154, 193, 132 and 115 s on book, table, math, twocol and form (single run). With PP-OCRv6 medium it took 41, 129, 153, 101 and 86 s.

Where the time goes (book page, default config, 55.6 s, single run): the OCR pipeline took 44.4 s (about 80%), layout detection 2.6 s, region detection 1.9 s, document preprocessor 1.1 s, table pipeline 0.6 s. Recognition cost scales with the number of text boxes, so the table page is far slower than the rest.

Selected configs (median s/page, CPU only, oneDNN off). Rows marked 2x1 are one pass on two pages; FINAL rows are 3 passes on five pages.

| Config | Pages x passes | book | table | math | twocol | form | Peak RSS (MiB) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| F0 default, seal/chart off | 1x1 | 55.6 | - | - | - | - | 3014 |
| F4 minimal, OCRv6 medium | 2x1 | 31.3 | 97.0 | - | - | - | 997 |
| Plain OCR, v6 medium, det limit 960 | 2x1 | 18.0 | 88.4 | - | - | - | 694 |
| FINAL structure minimal, v6 small | 5x3 | 8.0 | 32.2 | 11.7 | 17.7 | 25.7 | 1105 |
| FINAL structure minimal, v6 tiny | 5x3 | 4.0 | 10.9 | 5.0 | 6.2 | 8.5 | 765 |
| FINAL plain OCR, v6 tiny | 5x3 | 1.6 | 8.3 | 2.4 | 3.8 | 5.9 | 567 |

"Minimal" means orientation, unwarping, region detection, textline orientation, formula and table recognition are off, so only layout detection and text OCR run. Model size is the main lever. Layout detection costs a fixed 2.1 to 2.9 s per page. `cpu_threads` 16 instead of the default 10 changed nothing for tiny. The 960 px detection limit helped the medium model (detection 16.5 to 4.1 s on book) and made no measurable difference for tiny.

**Best config:** PP-StructureV3 minimal with PP-OCRv6 tiny det/rec and a detection limit of 960, at 4.0 to 10.9 s/page on this CPU (peak RSS 765 MiB). Individual module toggles were not isolated, and smaller layout models and PP-OCRv5 mobile were not tried.

**oneDNN failure.** With oneDNN on, paddlepaddle 3.3.1 CPU fails on PP-DocLayout_plus-L, PP-OCRv6 detection and the plain PaddleOCR pipeline with `NotImplementedError: ConvertPirAttribute2RuntimeAttribute not support [pir::ArrayAttribute<pir::DoubleAttribute>]`. All Fast-tier runs use `enable_mkldnn=False`. The PaddleOCR-VL pipeline ran with default settings without this error; that was not tested with both settings. PP-StructureV3 in paddleocr 3.7.0 also rejects `ocr_version="PP-OCRv6"`; naming the `PP-OCRv6_*_det/rec` models explicitly works.

**Caveat.** With table and formula recognition off, tables and display equations are output as images, not text. Table recognition on costs about 140 s on the table page.

## 6. Answers to the open questions in architecture.md

1. **DeepSeek-OCR 2 or v1 for Max.** v1. Multi-tile works by default for both models on b11669, so maturity did not separate them. Quality did not favor v2, v2 needs DRY on Q4_K_M, and only v1 has an official GGUF (section 4).
2. **Real speeds and memory.** See section 3. GPU: 1.6 to 11.4 s/page across models. CPU-16: DeepSeek v1 17 to 41 s, DeepSeek 2 23 to 48 s, PaddleOCR-VL 20 to 109 s. Single machine, 3 warm runs; no laptop CPU was measured.
3. **PP-DocLayoutV3 runtime.** It ran under paddlepaddle 3.3.1 CPU at about 2.8 s per page. ONNX Runtime was not tested.
4. **Vulkan coverage.** Not tested. No AMD or Intel GPU was available.
5. **Grounding syntax.** Confirmed. Each block is `<|ref|>LABEL<|/ref|><|det|>[[x1, y1, x2, y2]]<|/det|>` followed by the content, and the output ends with the end-of-sentence token. Coordinates are integers 0 to 999, normalized to the page image. Labels seen: `text, title, sub_title, table, image, image_caption, equation` (v1) and `text, title, sub_title, table, image, equation, formula_number` (v2); the full label list is unknown from five pages. Tables are HTML with rowspan/colspan and formulas use `\[ \]` and `\( \)`. The server must be started with `--special`, otherwise the tags are stripped. Passing `special` or `skip_special_tokens` in the request body does not work.

## 7. Required llama-server settings

DeepSeek-OCR (v1 and v2, same template name):

```
llama-server -m <model.gguf> --mmproj <mmproj.gguf> --host 127.0.0.1 --port <p> \
  --special --chat-template deepseek-ocr --no-jinja --temp 0 \
  --flash-attn off --no-warmup -c 8192 -np 1 -ngl 99
```

- `--special` is required for grounding tokens. The prompt is `<|grounding|>Convert the document to markdown.`
- DeepSeek 2 Q4_K_M needs `--dry-multiplier 0.8 --dry-base 1.75 --dry-allowed-length 2 --dry-penalty-last-n 8192 --dry-sequence-breaker none`. `--dry-penalty-last-n -1` is rejected by b11669.
- Dense pages generated up to about 1,900 tokens on top of 914 to 1132 prompt tokens, so `-c 8192` has headroom and `-c 4096` is the practical floor.
- Tiling is controlled by mmproj metadata keys (`clip.vision.preproc_min_tiles`, `preproc_max_tiles`, `preproc_image_size`), not by `--image-min-tokens` or `--image-max-tokens`, which had no effect. Forcing v1 to one tile made both dense pages degenerate and hit max_tokens. Leave the defaults.
- CPU: `-ngl 0`.

PaddleOCR-VL 1.6 (used through PaddleOCR's `llama-cpp-server` backend):

```
llama-server -m PaddleOCR-VL-1.6-GGUF.gguf --mmproj PaddleOCR-VL-1.6-GGUF-mmproj.gguf \
  --host 127.0.0.1 --port <p> --temp 0 -ngl 99 -np 1 -c 16384
```

- Always set `-np 1 -c 16384`. The defaults cost 11.5 GB of VRAM instead of 2.5 GB.
- No `--special` is needed. The image size limit lives in the mmproj metadata (`clip.vision.image_max_pixels`); the pipeline warns that it ignores `min_pixels` and `max_pixels`.

## 8. Decisions

Accepted on 2026-10-15:

1. **Max tier ships DeepSeek-OCR v1 Q8_0** from the official ggml-org GGUF repo (MIT), not DeepSeek-OCR 2. ADR 0004 and the Max row of the tier table are updated to match.
2. **DeepSeek-OCR 2 is an optional, experimental model under Max, not a tier default.** Advanced users can pick it (for example from an "Advanced models" section in Settings, or from the alternatives on the setup screen). It comes from the community conversion sabafallah/DeepSeek-OCR-2-GGUF, pinned in the manifest to revision d08e5af4 and the SHA-256 values in section 2 so it cannot change underneath us. It runs with DRY sampling enabled (needed to stop the Q4_K_M quant looping, section 7) and is labelled experimental in the UI. License: Apache-2.0 (upstream weights). Before v0.1 is released, we either verify that conversion or publish our own from the official deepseek-ai/DeepSeek-OCR-2 weights.
3. **Accurate tier (PaddleOCR-VL 1.6) is the default recommendation for most hardware.** It has the lowest memory floor (about 2.3 GB VRAM), gives structured output, and was the cleanest on the clean-render page. Max is recommended for GPUs with at least 8 GB of VRAM and for Apple Silicon Macs with at least 16 GB of memory.
4. **Fast tier is redefined** as "text and layout only; tables and formulas are kept as images in the output", using PP-StructureV3 with only layout detection and text OCR enabled, PP-OCRv6 tiny det/rec, a detection limit of 960, and oneDNN off. It measured 4 to 11 s/page on this CPU; a laptop will be slower. ONNX Runtime and OpenVINO evaluation is deferred until after v0.1.
5. **Suggester note.** On CPU-only machines with enough RAM, DeepSeek-OCR v1 (17 to 41 s/page on this CPU) was faster than PaddleOCR-VL on CPU (20 to 109 s/page). Suggester rules must not assume Accurate is faster than Max on CPU. This is one CPU at 16 threads; recheck on laptop hardware.
6. **No Apple Silicon benchmark for now.** The estimates in section 3 stand in until real numbers exist.

## 9. Follow-ups and not tested

- Vulkan on AMD and Intel GPUs (no hardware available).
- Apple Silicon and the Metal backend. Not measured; see the estimates at the end of section 3, which should be replaced with real numbers.
- Verifying the sabafallah DeepSeek-OCR 2 conversion, or publishing our own from the official weights, before v0.1 (section 8, item 2).
- Real laptop CPUs and an 8 GB GPU.
- PP-DocLayoutV3 through ONNX Runtime, and ONNX Runtime or OpenVINO for the Fast tier.
- More pages, to get statistically meaningful accuracy. Five pages with one run each cannot rank the engines.
- PaddleOCR-VL dropping equation numbers, and why DeepSeek v1 and DeepSeek 2 Q8 lose footnotes.
- Footnote handling in general, and quote normalization (straight versus curly) in the IR.
