use super::*;

fn workspace() -> JsonWorkspace {
    let mut workspace = JsonWorkspace::new(&egui::Context::default());
    let source = r#"{"a":1,"b":2}"#;
    workspace.source = Arc::from(source);
    workspace.document = Some(Arc::new(json::parse(source.into()).unwrap()));
    workspace.select_node(1);
    workspace
}

#[test]
fn unapplied_node_draft_counts_as_dirty_and_survives_selection_attempts() {
    let mut workspace = workspace();
    assert!(!workspace.is_dirty());
    workspace.editing_node = true;
    workspace.detail_text = "42".into();
    assert!(workspace.has_node_draft());
    assert!(workspace.is_dirty());
    workspace.select_node(2);
    assert_eq!(workspace.selected, Some(1));
    assert_eq!(workspace.detail_text, "42");
    assert!(workspace.action_error.is_some());
    workspace.mark_saved(r#"{"a":1,"b":2}"#);
    assert!(workspace.is_dirty());
    workspace.discard_node_draft();
    assert!(!workspace.has_node_draft());
    assert!(!workspace.is_dirty());
    assert_eq!(workspace.detail_text, "1");
}

#[test]
fn format_and_reparse_do_not_discard_node_drafts() {
    let mut workspace = workspace();
    workspace.editing_node = true;
    workspace.detail_text = "42".into();
    workspace.launch_format(true);
    assert!(workspace.work.is_none());
    workspace.launch_parse();
    assert!(workspace.work.is_none());
    assert_eq!(workspace.source.as_ref(), r#"{"a":1,"b":2}"#);
    assert_eq!(workspace.detail_text, "42");
}

#[test]
fn invalid_node_replacement_preserves_document_and_draft() {
    let mut workspace = workspace();
    workspace.editing_node = true;
    workspace.detail_text = "not-json".into();
    workspace.launch_replace(1, workspace.detail_text.clone());
    let deadline = Instant::now() + Duration::from_secs(3);
    while workspace.work.is_some() && Instant::now() < deadline {
        workspace.poll(&egui::Context::default());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(workspace.work.is_none());
    assert!(workspace.document.is_some());
    assert_eq!(workspace.source.as_ref(), r#"{"a":1,"b":2}"#);
    assert_eq!(workspace.detail_text, "not-json");
    assert!(workspace.has_node_draft());
    assert!(
        workspace
            .action_error
            .as_deref()
            .unwrap()
            .contains("原文未修改")
    );
}

#[test]
fn node_replacement_preserves_large_numbers_and_unrelated_members() {
    let mut workspace = workspace();
    workspace.launch_replace(1, "90071992547409931234567890".into());
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut events = Vec::new();
    while workspace.work.is_some() && Instant::now() < deadline {
        events.extend(workspace.poll(&egui::Context::default()));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        workspace.source.as_ref(),
        r#"{"a":90071992547409931234567890,"b":2}"#
    );
    assert!(workspace.is_dirty());
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], JsonEvent::Parsed { operation, .. } if operation == "修改节点"));
}

#[test]
fn text_buffer_retains_invalid_input_until_corrected() {
    let mut workspace = workspace();
    workspace.edit_text = "{unfinished".into();
    workspace.source_edited();
    workspace.launch_parse();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut events = Vec::new();
    while workspace.work.is_some() && Instant::now() < deadline {
        events.extend(workspace.poll(&egui::Context::default()));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(workspace.error.is_some());
    assert_eq!(workspace.source.as_ref(), "{unfinished");
    assert_eq!(workspace.edit_text, "{unfinished");
    assert!(events.is_empty());
}

#[test]
fn large_tree_frame_paints_a_viewport_instead_of_every_node() {
    let mut source = String::from("[");
    for index in 0..100_000 {
        if index > 0 {
            source.push(',');
        }
        source.push_str(&index.to_string());
    }
    source.push(']');
    let mut workspace = workspace();
    workspace.source = Arc::from(source.as_str());
    workspace.document = Some(Arc::new(json::parse(source).unwrap()));
    workspace.selected = None;
    workspace.select_node(0);
    workspace.view = View::Tree;
    workspace.expanded.insert(0);
    workspace.rows = (0..100_001)
        .map(|node| Row {
            node,
            depth: if node == 0 { 0 } else { 1 },
        })
        .collect();
    let ctx = egui::Context::default();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| workspace.show(ui));
        },
    );
    assert!(
        output.shapes.len() < 2_000,
        "painted {} shapes for a single viewport",
        output.shapes.len()
    );
}

#[test]
fn editable_source_frame_has_finite_layout() {
    let mut workspace = workspace();
    workspace.edit_text = workspace.source.to_string();
    workspace.view = View::Source;
    let ctx = egui::Context::default();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| workspace.show(ui));
        },
    );
    assert!(!output.shapes.is_empty());
}
