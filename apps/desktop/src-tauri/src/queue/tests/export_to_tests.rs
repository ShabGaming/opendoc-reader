//! Saving copies to a chosen path and opening outputs.

use std::fs;

use super::*;

async fn finished(rig: &Rig, name: &str) -> String {
    let id = rig.queue.add(vec![rig.doc(name)]).unwrap()[0]
        .file_id
        .clone();
    wait_for("inspection", || async {
        rig.item(&id).page_count.is_some()
    })
    .await;
    rig.queue.start(rig.options()).unwrap();
    wait_status(rig, &id, QueueStatus::Done).await;
    id
}

#[tokio::test]
async fn export_to_writes_the_exact_path_and_leaves_no_staging_folder() {
    let rig = Rig::new(FakeEnv::new().var("FAKE_FIGURE", "1"));
    let id = finished(&rig, "report.pdf").await;
    let out = rig.env.dir.path().join("out");
    fs::create_dir_all(&out).unwrap();

    let docx = out.join("Copy of report.docx");
    let saved = rig
        .queue
        .export_to(&id, ExportFormat::Docx, docx.to_str().unwrap(), false)
        .await
        .unwrap();
    assert_eq!(Path::new(&saved.path), docx);
    assert!(docx.is_file());

    let md = out.join("Copy of report.md");
    let saved = rig
        .queue
        .export_to(&id, ExportFormat::Markdown, md.to_str().unwrap(), false)
        .await
        .unwrap();
    assert!(md.is_file());
    let assets = out.join("Copy of report_assets");
    assert_eq!(saved.assets_dir.as_deref(), Some(assets.to_str().unwrap()));
    assert!(assets.join("p0-b0.png").is_file());

    let leftovers: Vec<_> = fs::read_dir(&out)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        leftovers.iter().all(|n| !n.starts_with(".opendoc-tmp")),
        "{leftovers:?}"
    );
    // A copy is not one of the conversion's outputs.
    assert_eq!(rig.item(&id).outputs.len(), 2);
}

#[tokio::test]
async fn export_to_refuses_to_overwrite_unless_told_to() {
    let rig = Rig::new(FakeEnv::new());
    let id = finished(&rig, "report.pdf").await;
    let dest = rig.env.dir.path().join("docs").join("mine.docx");
    fs::write(&dest, "precious").unwrap();

    let err = rig
        .queue
        .export_to(&id, ExportFormat::Docx, dest.to_str().unwrap(), false)
        .await
        .unwrap_err();
    assert_eq!(err.reason.as_deref(), Some("already_exists"));
    assert_eq!(fs::read_to_string(&dest).unwrap(), "precious");

    rig.queue
        .export_to(&id, ExportFormat::Docx, dest.to_str().unwrap(), true)
        .await
        .unwrap();
    assert_eq!(fs::read_to_string(&dest).unwrap(), "fake docx");
}

#[tokio::test]
async fn export_to_assembles_again_when_the_document_is_gone() {
    let rig = Rig::new(FakeEnv::new());
    let id = finished(&rig, "report.pdf").await;
    let _ = fs::remove_file(rig.svc.paths.document_json(&id));
    let dest = rig.env.dir.path().join("docs").join("again.docx");
    rig.queue
        .export_to(&id, ExportFormat::Docx, dest.to_str().unwrap(), false)
        .await
        .unwrap();
    assert!(dest.is_file());
}

#[tokio::test]
async fn export_to_needs_a_finished_file() {
    let rig = Rig::new(FakeEnv::new());
    let id = rig.queue.add(vec![rig.doc("a.pdf")]).unwrap()[0]
        .file_id
        .clone();
    let dest = rig.env.dir.path().join("docs").join("a.docx");
    let err = rig
        .queue
        .export_to(&id, ExportFormat::Docx, dest.to_str().unwrap(), false)
        .await
        .unwrap_err();
    assert_eq!(err.reason.as_deref(), Some("not_done"));
}

#[tokio::test]
async fn only_known_outputs_that_still_exist_can_be_opened() {
    let rig = Rig::new(FakeEnv::new());
    let id = finished(&rig, "report.pdf").await;
    let docx = rig.item(&id).outputs[0].path.clone();

    assert_eq!(rig.queue.known_output(&docx).unwrap(), PathBuf::from(&docx));
    let other = rig.env.dir.path().join("docs").join("report.pdf");
    let err = rig.queue.known_output(other.to_str().unwrap()).unwrap_err();
    assert_eq!(err.reason.as_deref(), Some("not_an_output"));

    assert!(rig
        .queue
        .check_outputs(&id)
        .unwrap()
        .iter()
        .all(|o| o.exists));
    fs::remove_file(&docx).unwrap();
    let err = rig.queue.known_output(&docx).unwrap_err();
    assert_eq!(err.reason.as_deref(), Some("file_missing"));
    let status = rig.queue.check_outputs(&id).unwrap();
    assert!(!status.iter().find(|o| o.path == docx).unwrap().exists);
    assert!(status.iter().any(|o| o.exists));
}
