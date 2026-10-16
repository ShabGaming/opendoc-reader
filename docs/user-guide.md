# OpenDoc Reader user guide

OpenDoc Reader turns scanned documents and photos of documents into Word (.docx) and Markdown (.md) files that you can edit. Everything runs on your computer. Your files are not uploaded anywhere.

This guide covers installing the app, the first run, converting files, checking the result, and what to do when something goes wrong. Version 0.1 is in testing, so some details may still change.

## Contents

- [Install](#install)
- [First run](#first-run)
- [Choosing a model](#choosing-a-model)
- [Converting files](#converting-files)
- [PDFs that already contain text](#pdfs-that-already-contain-text)
- [Reviewing a file](#reviewing-a-file)
- [Exporting](#exporting)
- [Where your files go](#where-your-files-go)
- [Settings](#settings)
- [Privacy](#privacy)
- [Troubleshooting](#troubleshooting)

## Install

Download the installer for your computer from the [Releases page](https://github.com/ShabGaming/opendoc-reader/releases).

- **Windows:** the `.exe` installer (or the `.msi` if your organization prefers it). You need Windows 10 version 22H2 or later, 64-bit.
- **macOS:** the `.dmg`. You need macOS 13 or later on an Apple Silicon Mac (M1 or newer). Intel Macs are not supported. Open the `.dmg` and drag OpenDoc Reader to Applications.

The installer is small, roughly 20 to 40 MB. The models and the document engine are downloaded the first time you run the app.

### Opening the app for the first time

Early releases are not code signed, so Windows and macOS show a warning the first time. The app is safe to open if you downloaded it from the Releases page above. Signed builds are planned before version 1.0.

**Windows.** If a blue "Windows protected your PC" window appears (SmartScreen):

1. Click **More info**.
2. Click **Run anyway**.

You only need to do this once per installer. If your organization blocks unsigned programs, ask your IT department to allow it.

**macOS.**

1. Open OpenDoc Reader from Applications. macOS says it cannot verify the app. Click **Done** (or **OK**).
2. Open **System Settings**, then **Privacy & Security**.
3. Scroll down to the message about OpenDoc Reader and click **Open Anyway**. Enter your password if asked.
4. Open the app again and click **Open**.

You only need to do this once. On macOS 15 and later, right-clicking the app and choosing Open no longer skips the warning, so use Privacy & Security.

## First run

The first time you open the app, a short setup walks you through four steps.

1. **Welcome.** Click **Get started**.
2. **Your computer.** The app looks at your graphics card, memory, processor and free disk space. Nothing leaves your computer.
3. **Choose a model.** The app recommends one for your computer. See [Choosing a model](#choosing-a-model). You can pick a different one, or more than one.
4. **Download.** The app downloads the model, the recognition runtime and the document engine. This happens once, and you need an internet connection for it. You can pause and resume. If you close the app or lose the connection, files that were already downloaded are kept, and the download continues when you try again. When it finishes, the app times a sample page so it can show you roughly how long a page takes on your computer.

Click **Start using OpenDoc Reader** when the download is done. After that the app works offline.

By default the models are stored in the app's data folder. Step 3 lets you choose another folder, for example on a drive with more free space. The folder can only be changed while no model is installed.

## Choosing a model

A model is the part of the app that reads the page. There are three tiers, with different trade-offs. You can install more than one and switch between them for each batch of files.

| Tier | Good for | What to know |
| --- | --- | --- |
| **Fast** | Plain text and simple layouts | Runs on any computer, even without a graphics card. Small download, about 140 MB. Tables and formulas are kept as pictures instead of being turned into editable tables and equations. |
| **Accurate** | Tables, formulas and complex layouts | The recommended model for most computers. Needs about 2.3 GB of graphics memory to run quickly. On a computer without a graphics card it works but is slow. The download is under 2 GB. |
| **Max** | Difficult scans on stronger hardware | The largest model. It needs a graphics card with at least 8 GB of memory, or an Apple Silicon Mac with at least 16 GB. The download is about 3.6 GB. "Max" means largest, not necessarily the most accurate. |

Speeds below are rough. They come from tests on one desktop PC, so a laptop will be slower. Apple Silicon speeds have not been measured; we estimate roughly 5 to 40 seconds per page. The app shows its own estimate for your computer after setup.

| Tier | Roughly, per page |
| --- | --- |
| Fast | 4 to 11 seconds on a fast desktop processor, longer on a laptop |
| Accurate | About 7 seconds on a high-end desktop graphics card. 20 to 110 seconds on a fast processor alone, depending on the page |
| Max | About 3 seconds on a high-end desktop graphics card. 17 to 40 seconds on a fast processor alone, so with no graphics card but plenty of memory it can be faster than Accurate |

The setup screen marks each model for your computer: **Runs well**, **Slow on this PC**, or **Won't fit** (not enough memory, so it cannot be installed). Pick **Model details** to see the exact model, its license and where it comes from.

Under **Advanced models** there is one experimental model for people who want to try it. It has no measured advantage over Max and needs about 7 GB of graphics memory.

You can add or remove models later in Settings, under Models.

## Converting files

1. Drop PDFs or images onto the window, or click **or choose files...**. You can also drop whole folders. The app reads PDF, PNG, JPG, TIFF (including multi-page), BMP, WebP and HEIC files.
2. The files appear in the queue on the left. Click one to see its details: type, number of pages and size.
3. At the bottom, choose:
   - **Format:** Word, Markdown, or both. At least one stays checked.
   - **Model:** which installed model to use.
   - **Save to:** next to the original file (the default), or a folder you choose.
4. Click **Convert**. The label shows how many files will start, for example "Convert 3 files".

Files convert one at a time in the background. Each shows its page progress and an estimate of the time left. While a batch runs you can:

- **Pause all** and **Resume all**. The page being read finishes first.
- **Cancel all**. Pages that were already finished are kept.
- Add more files to the queue.

If you close the app in the middle of a batch, it picks up again from the last finished page the next time you open it.

Converted files are saved with the original name and a new extension, so `invoice.pdf` becomes `invoice.docx` and `invoice.md`. Existing files are never overwritten. If `invoice.docx` already exists, the new file is called `invoice (2).docx`.

## PDFs that already contain text

Many PDFs are not scans. Their text is stored in the file, and no reading is needed. When you select such a PDF, the app tells you: **This PDF already contains text.** It offers two choices:

- **Use existing text.** Instant and exact, because the app copies the text that is already there and arranges it into headings, paragraphs and lists. Tables, formulas and pictures are not recreated this way.
- **Run OCR anyway.** The app reads the page images with the model instead. Choose this when the stored text is wrong, for example in a scan that someone ran through poor OCR before.

The notice appears when at least half of the PDF's pages have text.

## Reviewing a file

When a file is done, click **Review** to check it page by page. The screen has:

- **Three views** at the top: **Original** (the scanned page), **Side by side** (the default), and **Result** (the converted text). Side by side and Result are available once the file is done.
- A strip of **page thumbnails** and **Previous page** and **Next page** buttons. Use **Zoom out** and **Zoom in** to change the size.
- **Blocks.** The converter splits each page into blocks: headings, paragraphs, lists, tables, formulas, figures and captions. With Blocks on, each block on the original page has an outline, and each block in the result is labeled by type. Hover over a block on one side and the matching block lights up on the other side, so you can see what the app read from where. Switch Blocks off to hide the outlines and labels.
- Running heads and page numbers are marked in the result as **Left out of the export**. They are not copied into the Word or Markdown file.

In version 0.1 the review screen is read-only. To fix a word, click **Open in Word** (or **Open Markdown**) to open the saved file, and edit it there.

## Exporting

When a file finishes, it is saved in the formats you checked. A message tells you where, and **Show in folder** opens the folder.

The same buttons appear in Review and on the file's page in the queue: **Open in Word** opens the saved Word file, **Open Markdown** opens the Markdown file, and **Show in folder** shows it in your file manager. If you chose only one format, the main button is the one you have.

**Save a copy as...** writes a Word or Markdown file wherever you choose, in either format, including the one you did not pick before. It opens a save dialog that starts in the original's folder; the dialog asks before it replaces a file.

If you move or delete a saved file, the app says so and offers Save a copy as... to write it again.

What to expect in the files:

- **Word:** headings, paragraphs, lists, tables (with merged cells), pictures, captions and equations. Each source page starts on a new page. Footnotes appear at the end of their page.
- **Markdown:** the same content as text. Pictures are saved in a folder next to the file called `name_assets`, and the Markdown links to them, so keep the two together when you move the file.
- Fonts and exact positions are approximated, because the goal is a file that is easy to edit.
- With the Fast tier, tables and formulas are pictures.

## Where your files go

- **Results** go next to the original file, or in the folder you picked under **Save to**.
- **Models, the document engine, work files, settings and logs** are in one folder named `OpenDoc Reader`. On Windows it is `%LOCALAPPDATA%\OpenDoc Reader` (usually `C:\Users\<you>\AppData\Local\OpenDoc Reader`), and on macOS it is `~/Library/Application Support/OpenDoc Reader`. Models can live elsewhere if you chose another folder during setup.
- **Logs** are in the `logs` folder inside it. Use **Open logs folder** in Settings to open it.
- If you used an earlier build, the app moves its old folder (`io.github.shabgaming.opendocreader` under `%APPDATA%` or `~/Library/Application Support/`) to the new place the first time it starts. Large models can take a few minutes to move to another drive. If the move fails, the app keeps using the old folder and says why in the log.

To free space, remove models you do not use in Settings, under Models.

## Settings

Open Settings with the gear at the bottom of the queue.

**General**

- Default format and default model. They apply to new files, and you can change them for each batch.
- Save location: next to the original, or always one folder.
- Theme: System, Light or Dark.
- Check for new versions once a month. This only asks GitHub whether a newer version exists. If there is one, the app tells you. It never updates itself.
- Use Vulkan for AMD and Intel graphics. Experimental and untested. Without it, computers with an AMD or Intel graphics card use the processor.

**Models**

- Installed and available models, with size and license. Install or remove them here.
- The models folder. It can only be changed while no model is installed.
- **Import model from folder**, for computers that cannot download. The files are checked before use.
- Advanced models.

**Privacy and logs**

- Open the logs folder.
- **Copy diagnostics**, which copies the app version, a short system summary and recent log lines, so you can paste them into a bug report. Nothing is sent anywhere.

**About** shows the version, the link to the source code and the licenses of the app, its dependencies and the models. The full list is in [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md). It also lists the project's contributors with their GitHub avatars, most contributions first; clicking a name opens that person's GitHub page in your browser.

## Privacy

OpenDoc Reader has no accounts, no cloud service and no telemetry. Your documents and results stay on your computer.

The app uses the internet only to:

- download models, the recognition runtime and the document engine's packages during setup or when you install a model. Every file is checked against a known checksum. The app uses your system's proxy settings.
- ask GitHub once a month whether a newer version exists, if you leave **Check for new versions** on. Nothing about you or your files is sent.

The contributor list in About is bundled with the app when it is built. Nothing is fetched to show it, and no avatar is loaded from the internet.

After setup, you can use the app with no connection.

## Troubleshooting

**Windows says "Windows protected your PC", or macOS says the app cannot be opened.**
The build is not signed yet. See [Opening the app for the first time](#opening-the-app-for-the-first-time).

**The download fails or never starts.**
Check your connection and click **Retry**. Files that were already downloaded are kept, so you do not start over. Company networks, school networks and some security programs block the download sites. The app downloads models from Hugging Face (with ModelScope as a backup), the recognition runtime from GitHub, and the document engine's packages from the Python package index. If your network blocks them, try another network, ask your IT department to allow them, or use the next item.

**My computer cannot download at all.**
Use **Settings, Models, Import model from folder**. On a computer that has access, download the model's files. **Model details** in setup lists where each model comes from. Copy them to a folder, then pick that folder on the computer that cannot download. The app checks each file before using it, and says if a file is missing or does not match.

**A download says the checksum did not match.**
The app discards the file. Click **Retry** to download it again. If it keeps happening, the network may be changing the file on the way. Try a different network.

**The app says there is not enough free space.**
Free up space on the drive it names, or pick another folder for the models during setup. In Settings, Models, you can remove models you do not need.

**The computer ran out of memory, or the model could not be loaded.**
Install a smaller model and convert the file again. For example, use Accurate instead of Max, or Fast instead of Accurate. Closing other programs also helps. Very large pages and long PDFs need more memory.

**Conversion is very slow.**
A computer without a supported graphics card reads pages much more slowly. Fast is the quickest tier on a processor. For a large batch, start it before you leave the computer for the night, because the queue continues in the background and resumes if the app restarts.

**A file failed.**
Click **Retry** or **Add again**. The message says why when it knows. A password-protected PDF has to be unlocked first, and a damaged file may need a new copy. If it keeps failing, report it with the diagnostics (see below).

**The text has mistakes.**
Scans with low resolution, skew, shadows or handwriting are hard to read. Try a larger tier, or scan again at 200 to 300 DPI. Review shows what the app read from where, which helps you spot the problem areas.

**Where do I find the logs?**
Settings, Privacy and logs, then **Open logs folder**.

**How do I report a problem?**
Open Settings, Privacy and logs, click **Copy diagnostics**, and paste the text into a new issue at [github.com/ShabGaming/opendoc-reader/issues](https://github.com/ShabGaming/opendoc-reader/issues). Say which model you used and what you did before the problem. Check the pasted text first and remove anything you do not want to share. The diagnostics do not contain your documents.
