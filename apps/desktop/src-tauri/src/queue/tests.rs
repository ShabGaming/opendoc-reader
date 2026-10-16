//! Queue tests against the fake engine: no models, no GPU, no real engine.

use std::future::Future;
use std::time::Duration;

use super::*;
use crate::contract::SaveLocation;
use crate::fakes::{read_fake_log, test_services, CollectSink, FakeEnv};
use crate::models::tests::tiny_model_manifest;

mod export_to_tests;

struct Rig {
    env: FakeEnv,
    svc: Arc<Services>,
    queue: Arc<Queue>,
    sink: Arc<CollectSink>,
}

impl Rig {
    /// A queue over the fake engine with the model `tiny` installed.
    fn new(env: FakeEnv) -> Self {
        let (svc, sink) = test_services(&env, tiny_model_manifest());
        let model_dir = svc.models_dir().join("tiny");
        std::fs::create_dir_all(model_dir.join("sub")).unwrap();
        std::fs::write(model_dir.join("sub").join("a.bin"), "alpha").unwrap();
        std::fs::write(model_dir.join("b.bin"), "bravo!").unwrap();
        let queue = Queue::new(svc.clone()).unwrap();
        tokio::spawn(queue.clone().inspect_loop());
        tokio::spawn(queue.clone().run_loop());
        Self {
            env,
            svc,
            queue,
            sink,
        }
    }

    fn doc(&self, relative: &str) -> String {
        let path = self.env.dir.path().join("docs").join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"%PDF-fake").unwrap();
        path.to_string_lossy().into_owned()
    }

    fn options(&self) -> ConvertOptions {
        ConvertOptions {
            formats: vec![ExportFormat::Docx, ExportFormat::Markdown],
            model_id: "tiny".into(),
            save_to: SaveLocation::Original,
        }
    }

    fn item(&self, file_id: &str) -> QueueItem {
        self.queue
            .list()
            .unwrap()
            .into_iter()
            .find(|i| i.file_id == file_id)
            .unwrap()
    }

    fn methods(&self) -> Vec<String> {
        read_fake_log(&self.env)
            .iter()
            .map(|e| e["method"].as_str().unwrap().to_string())
            .collect()
    }

    fn count(&self, method: &str) -> usize {
        self.methods().iter().filter(|m| *m == method).count()
    }
}

async fn wait_for<F: Future<Output = bool>>(what: &str, mut condition: impl FnMut() -> F) {
    for _ in 0..400 {
        if condition().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for: {what}");
}

async fn wait_status(rig: &Rig, file_id: &str, status: QueueStatus) {
    wait_for(&format!("{file_id} to be {status:?}"), || async {
        rig.item(file_id).status == status
    })
    .await;
}

fn write_ir(rig: &Rig, file_id: &str, page: u32) {
    let path = rig.svc.paths.page_ir(file_id, page);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        json!({"index": page, "width": 8, "height": 8, "sourceImage": "x.png", "blocks": []})
            .to_string(),
    )
    .unwrap();
}

#[tokio::test]
async fn added_files_are_inspected_and_get_a_preview() {
    let rig = Rig::new(FakeEnv::new());
    let items = rig.queue.add(vec![rig.doc("scan.pdf")]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, QueueStatus::Waiting);
    assert_eq!(items[0].name, "scan.pdf");
    assert_eq!(items[0].kind, FileKind::Pdf);
    let id = items[0].file_id.clone();

    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;
    let item = rig.item(&id);
    assert_eq!(item.page_count, Some(3));
    assert_eq!(item.has_text_layer, Some(false));
    assert_eq!(item.status, QueueStatus::Waiting);
    assert!(Path::new(item.preview_path.as_deref().unwrap()).is_file());
    assert!(item.preview_path.unwrap().ends_with("page-0000.png"));
}

#[tokio::test]
async fn a_pdf_with_text_asks_the_text_question() {
    let rig = Rig::new(FakeEnv::new().var("FAKE_TEXT_LAYER", "1"));
    let id = rig.queue.add(vec![rig.doc("born-digital.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_status(&rig, &id, QueueStatus::NeedsAttention).await;
    let item = rig.item(&id);
    assert_eq!(item.attention, Some(AttentionReason::HasTextLayer));
    assert_eq!(item.has_text_layer, Some(true));

    rig.queue
        .set_text_mode(&id, TextMode::ExistingText)
        .unwrap();
    let item = rig.item(&id);
    assert_eq!(item.status, QueueStatus::Waiting);
    assert_eq!(item.text_mode, Some(TextMode::ExistingText));
    assert_eq!(item.attention, None);
    // The question can only be answered once.
    assert!(rig.queue.set_text_mode(&id, TextMode::Ocr).is_err());
}

#[tokio::test]
async fn an_unusable_file_fails_at_inspection_with_the_engines_reason() {
    let rig = Rig::new(FakeEnv::new().var("FAKE_INSPECT_FAIL", "password_protected"));
    let id = rig.queue.add(vec![rig.doc("locked.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_status(&rig, &id, QueueStatus::Failed).await;
    let error = rig.item(&id).error.unwrap();
    assert_eq!(error.code, AppErrorCode::BadInput);
    assert_eq!(error.reason.as_deref(), Some("password_protected"));
}

#[tokio::test]
async fn adding_skips_unsupported_files_and_duplicates_and_walks_folders() {
    let rig = Rig::new(FakeEnv::new());
    let notes = rig.env.dir.path().join("notes.txt");
    std::fs::write(&notes, "x").unwrap();
    let error = rig
        .queue
        .add(vec![notes.to_string_lossy().into_owned()])
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::BadInput);
    assert_eq!(error.reason.as_deref(), Some("unsupported_format"));

    let first = rig.doc("batch/a.pdf");
    rig.doc("batch/nested/b.pdf");
    std::fs::write(rig.env.dir.path().join("docs/batch/readme.txt"), "x").unwrap();
    let folder = rig
        .env
        .dir
        .path()
        .join("docs/batch")
        .to_string_lossy()
        .into_owned();
    let added = rig.queue.add(vec![folder.clone()]).unwrap();
    assert_eq!(added.len(), 2);
    assert_eq!(
        added.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        vec!["a.pdf", "b.pdf"]
    );
    // The same paths again are not added a second time.
    assert!(rig.queue.add(vec![first, folder]).unwrap().is_empty());
    assert_eq!(rig.queue.list().unwrap().len(), 2);
}

#[tokio::test]
async fn a_conversion_runs_every_page_then_exports_next_to_the_original() {
    let rig = Rig::new(FakeEnv::new());
    let source = rig.doc("invoice.pdf");
    let id = rig.queue.add(vec![source.clone()]).unwrap()[0]
        .file_id
        .clone();
    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;

    rig.queue.start(rig.options()).unwrap();
    wait_status(&rig, &id, QueueStatus::Done).await;

    let item = rig.item(&id);
    assert_eq!(item.pages_done, 3);
    assert!(item.duration_ms.is_some());
    assert_eq!(item.model_id.as_deref(), Some("tiny"));
    assert_eq!(item.text_mode, Some(TextMode::Ocr));
    let dir = rig.env.dir.path().join("docs");
    assert_eq!(item.outputs.len(), 2);
    assert_eq!(item.outputs[0].format, ExportFormat::Docx);
    assert_eq!(Path::new(&item.outputs[0].path), dir.join("invoice.docx"));
    assert_eq!(Path::new(&item.outputs[1].path), dir.join("invoice.md"));
    assert!(dir.join("invoice.docx").is_file());
    assert!(dir.join("invoice.md").is_file());

    let methods = rig.methods();
    let from = methods.iter().position(|m| m == "convert.page").unwrap();
    assert_eq!(
        &methods[from..],
        [
            "convert.page",
            "page.render",
            "convert.page",
            "page.render",
            "convert.page",
            "document.assemble",
            "export.docx",
            "export.markdown"
        ]
    );

    let pages = rig.queue.pages(&id).unwrap();
    assert!(pages
        .iter()
        .all(|p| p.state == PageState::Done && p.image_path.is_some()));
    let review = rig.queue.review(&id, 1).await.unwrap();
    assert_eq!(review.page_index, 1);
    assert_eq!(review.page_count, 3);
    assert_eq!(review.page["blocks"][0]["text"], "Page 2");
    assert_eq!(review.engine.model, "tiny");
    assert!(rig.queue.review(&id, 9).await.is_err());
}

#[tokio::test]
async fn progress_events_cover_the_conversion_and_are_full_snapshots() {
    let rig = Rig::new(FakeEnv::new());
    let id = rig.queue.add(vec![rig.doc("a.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;
    rig.queue.start(rig.options()).unwrap();
    wait_status(&rig, &id, QueueStatus::Done).await;

    let events = rig.sink.events_named(EVENT_QUEUE_PROGRESS);
    let statuses: Vec<String> = events
        .iter()
        .map(|(_, p)| p["item"]["status"].as_str().unwrap().to_string())
        .collect();
    assert!(statuses.contains(&"converting".to_string()));
    assert_eq!(statuses.last().map(String::as_str), Some("done"));
    let finished_pages = events
        .iter()
        .filter(|(_, p)| p["page"]["state"] == "done")
        .count();
    assert_eq!(finished_pages, 3);
    for (_, payload) in &events {
        assert_eq!(payload["item"]["fileId"], id.as_str());
        assert!(payload["item"].get("outputs").is_some());
    }
    let last = &events.last().unwrap().1;
    assert_eq!(last["item"]["outputs"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_second_export_never_overwrites_the_first() {
    let rig = Rig::new(FakeEnv::new());
    let id = rig.queue.add(vec![rig.doc("report.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;
    rig.queue.start(rig.options()).unwrap();
    wait_status(&rig, &id, QueueStatus::Done).await;

    let again = rig.queue.export(&id, ExportFormat::Docx).await.unwrap();
    let dir = rig.env.dir.path().join("docs");
    assert_eq!(Path::new(&again.path), dir.join("report (2).docx"));
    let third = rig.queue.export(&id, ExportFormat::Docx).await.unwrap();
    assert_eq!(Path::new(&third.path), dir.join("report (3).docx"));
    assert!(dir.join("report.docx").is_file());
    assert_eq!(rig.item(&id).outputs.len(), 4);

    let md = rig.queue.export(&id, ExportFormat::Markdown).await.unwrap();
    assert_eq!(Path::new(&md.path), dir.join("report (2).md"));
}

#[tokio::test]
async fn exports_go_to_the_chosen_folder_when_one_is_set() {
    let rig = Rig::new(FakeEnv::new());
    let id = rig.queue.add(vec![rig.doc("a.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;
    let target = rig.env.dir.path().join("converted").join("here");
    rig.queue
        .start(ConvertOptions {
            save_to: SaveLocation::Folder {
                path: target.to_string_lossy().into_owned(),
            },
            ..rig.options()
        })
        .unwrap();
    wait_status(&rig, &id, QueueStatus::Done).await;
    assert!(target.join("a.docx").is_file());
    assert!(target.join("a.md").is_file());
}

#[tokio::test]
async fn pages_with_a_result_are_not_converted_again() {
    let rig = Rig::new(FakeEnv::new());
    let id = rig.queue.add(vec![rig.doc("resume.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;
    write_ir(&rig, &id, 0);
    write_ir(&rig, &id, 1);

    rig.queue.start(rig.options()).unwrap();
    wait_status(&rig, &id, QueueStatus::Done).await;

    assert_eq!(
        rig.count("convert.page"),
        1,
        "only the missing page is converted"
    );
    assert_eq!(rig.item(&id).pages_done, 3);
}

#[tokio::test]
async fn a_failed_page_fails_the_file_and_a_retry_resumes_after_the_finished_pages() {
    let rig = Rig::new(
        FakeEnv::new()
            .var("FAKE_FAIL_PAGE", "1")
            .var("FAKE_FAIL_CODE", "-32008"),
    );
    let id = rig.queue.add(vec![rig.doc("bad.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;
    rig.queue.start(rig.options()).unwrap();
    wait_status(&rig, &id, QueueStatus::Failed).await;

    let item = rig.item(&id);
    assert_eq!(item.error.unwrap().code, AppErrorCode::RecognitionFailed);
    assert_eq!(item.pages_done, 1);
    let pages = rig.queue.pages(&id).unwrap();
    assert_eq!(pages[0].state, PageState::Done);
    assert_eq!(pages[1].state, PageState::Failed);
    assert_eq!(
        pages[1].error.as_ref().unwrap().code,
        AppErrorCode::RecognitionFailed
    );
    assert_eq!(pages[2].state, PageState::Pending);
    assert!(rig.item(&id).outputs.is_empty());

    // Retrying queues it again with the old options and starts at the failed page.
    rig.queue.retry(&id).unwrap();
    wait_for("the second failure", || async {
        rig.count("convert.page") == 3 && rig.item(&id).status == QueueStatus::Failed
    })
    .await;
    assert_eq!(rig.count("convert.page"), 3, "page 0 once, page 1 twice");
    assert!(rig.queue.retry("no-such-file").is_err());
}

#[tokio::test]
async fn only_failed_or_cancelled_items_can_be_retried() {
    let rig = Rig::new(FakeEnv::new());
    let id = rig.queue.add(vec![rig.doc("a.pdf")]).unwrap()[0]
        .file_id
        .clone();
    let error = rig.queue.retry(&id).unwrap_err();
    assert_eq!(error.code, AppErrorCode::InvalidArgument);
}

#[tokio::test]
async fn cancel_stops_the_page_in_flight_and_keeps_the_finished_ones() {
    let rig = Rig::new(FakeEnv::new().var("FAKE_SLOW_MS", "4000"));
    let id = rig.queue.add(vec![rig.doc("slow.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;
    write_ir(&rig, &id, 0);
    rig.queue.start(rig.options()).unwrap();
    wait_for("the page to start", || async {
        rig.item(&id).status == QueueStatus::Converting && rig.count("convert.page") == 1
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let began = Instant::now();
    rig.queue.cancel(Some(id.clone())).await.unwrap();
    wait_status(&rig, &id, QueueStatus::Cancelled).await;
    assert!(began.elapsed() < Duration::from_secs(3));
    assert_eq!(rig.item(&id).pages_done, 1);
    assert!(rig.item(&id).outputs.is_empty());
    assert!(rig.item(&id).error.is_none());
    assert_eq!(rig.queue.pages(&id).unwrap()[0].state, PageState::Done);
}

#[tokio::test]
async fn cancel_all_cancels_waiting_and_paused_items_directly() {
    let rig = Rig::new(FakeEnv::new());
    let ids: Vec<String> = rig
        .queue
        .add(vec![rig.doc("a.pdf"), rig.doc("b.pdf")])
        .unwrap()
        .into_iter()
        .map(|i| i.file_id)
        .collect();
    rig.queue.cancel(None).await.unwrap();
    for id in &ids {
        assert_eq!(rig.item(id).status, QueueStatus::Cancelled);
    }
    // A cancelled item cannot be cancelled by name again.
    assert!(rig.queue.cancel(Some(ids[0].clone())).await.is_err());
}

#[tokio::test]
async fn pause_stops_after_the_page_and_resume_continues() {
    let rig = Rig::new(FakeEnv::new().var("FAKE_SLOW_MS", "400"));
    let id = rig.queue.add(vec![rig.doc("p.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;
    rig.queue.start(rig.options()).unwrap();
    wait_for("the first page", || async {
        rig.count("convert.page") == 1
    })
    .await;

    rig.queue.pause();
    wait_status(&rig, &id, QueueStatus::Paused).await;
    assert_eq!(
        rig.item(&id).pages_done,
        1,
        "the page in flight finished first"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(rig.count("convert.page"), 1, "nothing runs while paused");

    rig.queue.resume();
    wait_status(&rig, &id, QueueStatus::Done).await;
    assert_eq!(rig.count("convert.page"), 3);
}

#[tokio::test]
async fn files_run_one_at_a_time_in_queue_order() {
    let rig = Rig::new(FakeEnv::new());
    let ids: Vec<String> = rig
        .queue
        .add(vec![rig.doc("1.pdf"), rig.doc("2.pdf")])
        .unwrap()
        .into_iter()
        .map(|i| i.file_id)
        .collect();
    wait_for("inspection", || async {
        ids.iter().all(|id| rig.item(id).page_count.is_some())
    })
    .await;
    rig.queue.start(rig.options()).unwrap();
    wait_status(&rig, &ids[1], QueueStatus::Done).await;

    // While file 1 converts, file 2 never reports converting.
    let mut first_done = false;
    for (_, payload) in rig.sink.events_named(EVENT_QUEUE_PROGRESS) {
        let item = &payload["item"];
        if item["fileId"] == ids[0].as_str() && item["status"] == "done" {
            first_done = true;
        }
        if item["fileId"] == ids[1].as_str() && item["status"] == "converting" {
            assert!(
                first_done,
                "the second file started before the first finished"
            );
        }
    }
    assert!(first_done);
}

#[tokio::test]
async fn items_that_need_attention_wait_for_an_answer() {
    let rig = Rig::new(FakeEnv::new().var("FAKE_TEXT_LAYER", "1"));
    let id = rig.queue.add(vec![rig.doc("text.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_status(&rig, &id, QueueStatus::NeedsAttention).await;
    rig.queue.start(rig.options()).unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(rig.item(&id).status, QueueStatus::NeedsAttention);
    assert_eq!(rig.count("convert.page"), 0);

    // Answering after the run was started queues it with the same options.
    rig.queue
        .set_text_mode(&id, TextMode::ExistingText)
        .unwrap();
    wait_status(&rig, &id, QueueStatus::Done).await;
    assert_eq!(rig.count("text.extract"), 3);
    assert_eq!(rig.count("convert.page"), 0);
    let render = read_fake_log(&rig.env)
        .into_iter()
        .rfind(|e| e["method"] == "page.render")
        .unwrap();
    assert_eq!(render["params"]["rotate"], 0);
    assert_eq!(render["params"]["deskew"], false);
    assert_eq!(rig.item(&id).text_mode, Some(TextMode::ExistingText));
}

#[tokio::test]
async fn a_crash_during_a_page_is_recovered_by_the_supervisor() {
    let rig = Rig::new(FakeEnv::new().var("FAKE_CRASH_ONCE_ON", "convert.page"));
    let id = rig.queue.add(vec![rig.doc("crash.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;
    rig.queue.start(rig.options()).unwrap();
    wait_status(&rig, &id, QueueStatus::Done).await;
    assert_eq!(rig.item(&id).pages_done, 3);
}

#[tokio::test]
async fn starting_with_a_model_that_is_not_installed_is_rejected() {
    let rig = Rig::new(FakeEnv::new());
    std::fs::remove_dir_all(rig.svc.models_dir().join("tiny")).unwrap();
    let error = rig.queue.start(rig.options()).unwrap_err();
    assert_eq!(error.code, AppErrorCode::ModelNotReady);
    let error = rig
        .queue
        .start(ConvertOptions {
            model_id: "nope".into(),
            ..rig.options()
        })
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::InvalidArgument);
}

#[tokio::test]
async fn removing_and_clearing_delete_job_folders_but_not_outputs() {
    let rig = Rig::new(FakeEnv::new());
    let ids: Vec<String> = rig
        .queue
        .add(vec![rig.doc("keep.pdf"), rig.doc("drop.pdf")])
        .unwrap()
        .into_iter()
        .map(|i| i.file_id)
        .collect();
    wait_for("inspection", || async {
        ids.iter().all(|id| rig.item(id).page_count.is_some())
    })
    .await;
    rig.queue.start(rig.options()).unwrap();
    wait_status(&rig, &ids[1], QueueStatus::Done).await;
    assert!(rig.svc.paths.job_dir(&ids[0]).is_dir());

    rig.queue.remove(&ids[0]).unwrap();
    assert!(!rig.svc.paths.job_dir(&ids[0]).exists());
    assert_eq!(rig.queue.list().unwrap().len(), 1);

    let removed = rig.queue.clear_done().unwrap();
    assert_eq!(removed, vec![ids[1].clone()]);
    assert!(rig.queue.list().unwrap().is_empty());
    assert!(rig.env.dir.path().join("docs/drop.docx").is_file());
    assert!(rig.queue.remove("gone").is_err());
}

#[tokio::test]
async fn a_converting_item_cannot_be_removed() {
    let rig = Rig::new(FakeEnv::new().var("FAKE_SLOW_MS", "1500"));
    let id = rig.queue.add(vec![rig.doc("busy.pdf")]).unwrap()[0]
        .file_id
        .clone();
    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;
    rig.queue.start(rig.options()).unwrap();
    wait_status(&rig, &id, QueueStatus::Converting).await;
    let error = rig.queue.remove(&id).unwrap_err();
    assert_eq!(error.code, AppErrorCode::InvalidArgument);
    assert_eq!(rig.queue.models_in_use(), vec!["tiny".to_string()]);
    rig.queue.cancel(None).await.unwrap();
    wait_status(&rig, &id, QueueStatus::Cancelled).await;
    assert!(rig.queue.models_in_use().is_empty());
}

#[tokio::test]
async fn the_queue_survives_a_restart() {
    let env = FakeEnv::new();
    let (svc, _sink) = test_services(&env, tiny_model_manifest());
    let db_path = svc.paths.database.clone();

    // Rows as an app killed mid-conversion would leave them.
    let db = Db::open(&db_path).unwrap();
    let mut converting = db::sample_row("f-conv", "a.pdf");
    converting.status = QueueStatus::Converting;
    converting.page_count = Some(3);
    converting.pages_done = 2; // stale: only one IR file exists
    db.insert(&mut converting).unwrap();
    let mut paused = db::sample_row("f-paused", "b.pdf");
    paused.status = QueueStatus::Paused;
    db.insert(&mut paused).unwrap();
    let mut done = db::sample_row("f-done", "c.pdf");
    done.status = QueueStatus::Done;
    done.page_count = Some(2);
    done.pages_done = 2;
    db.insert(&mut done).unwrap();
    drop(db);

    let ir = svc.paths.page_ir("f-conv", 0);
    std::fs::create_dir_all(ir.parent().unwrap()).unwrap();
    std::fs::write(&ir, r#"{"blocks": []}"#).unwrap();

    let queue = Queue::new(svc).unwrap();
    let items = queue.list().unwrap();
    let by_id = |id: &str| items.iter().find(|i| i.file_id == id).unwrap();
    assert_eq!(by_id("f-conv").status, QueueStatus::Waiting);
    assert_eq!(
        by_id("f-conv").pages_done,
        1,
        "counted again from the IR files"
    );
    assert_eq!(by_id("f-paused").status, QueueStatus::Waiting);
    assert_eq!(by_id("f-done").status, QueueStatus::Done);
    assert_eq!(by_id("f-done").pages_done, 2);
}
