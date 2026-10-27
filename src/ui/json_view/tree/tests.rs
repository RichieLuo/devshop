use super::*;

fn workspace() -> JsonWorkspace {
    let ctx = egui::Context::default();
    let items: Vec<_> = (0..160)
        .map(|index| {
            serde_json::json!({
                "value": index,
                "label": if index == 120 { "unique-target" } else { "ordinary" },
            })
        })
        .collect();
    let source = serde_json::json!({"items": items}).to_string();
    let mut workspace = JsonWorkspace::new(&ctx);
    workspace.source = Arc::from(source.as_str());
    workspace.document = Some(Arc::new(json::parse(source).unwrap()));
    workspace.expanded.extend([0, 1]);
    workspace.select_node(1);
    workspace.rebuild_rows();
    finish_rows(&mut workspace);
    workspace
}

fn finish_rows(workspace: &mut JsonWorkspace) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while workspace.row_task.is_some() && Instant::now() < deadline {
        workspace.poll_rows();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(workspace.row_task.is_none());
}

fn frame(workspace: &mut JsonWorkspace) -> (egui::Id, egui::scroll_area::State) {
    let ctx = workspace.context.clone();
    let mut scroll_id = None;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 600.0),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                scroll_id = Some(ui.make_persistent_id(egui::Id::new("json-tree-scroll")));
                workspace.show_tree(ui);
            });
        },
    );
    let id = scroll_id.unwrap();
    (id, egui::scroll_area::State::load(&ctx, id).unwrap())
}

fn scroll_down(workspace: &mut JsonWorkspace) -> f32 {
    let (id, mut state) = frame(workspace);
    state.offset.y = 900.0;
    state.store(&workspace.context, id);
    let offset = frame(workspace).1.offset.y;
    assert!((offset - 900.0).abs() < 0.1);
    offset
}

#[test]
fn tree_scroll_stays_in_place_when_other_nodes_expand_and_collapse() {
    let mut workspace = workspace();
    let before = scroll_down(&mut workspace);
    let document = workspace.document.clone().unwrap();
    let nodes: Vec<_> = document.children(1).skip(32).take(3).collect();
    let original_rows = workspace.rows.len();
    for expanded in [true, false] {
        for &id in &nodes {
            if expanded {
                workspace.expanded.insert(id);
            } else {
                workspace.expanded.remove(&id);
            }
            workspace.rebuild_rows();
            assert!((frame(&mut workspace).1.offset.y - before).abs() < 0.1);
            finish_rows(&mut workspace);
            let after = frame(&mut workspace).1.offset.y;
            assert!(
                (after - before).abs() < 0.1,
                "scroll jumped from {before} to {after}"
            );
            assert_eq!(workspace.selected, Some(1));
        }
    }
    assert_eq!(workspace.rows.len(), original_rows);
}

#[test]
fn tree_scroll_still_reveals_search_results_after_async_expansion() {
    let mut workspace = workspace();
    for needs_expansion in [true, false] {
        let before = scroll_down(&mut workspace);
        workspace.query = "unique-target".into();
        workspace.launch_search();
        let deadline = Instant::now() + Duration::from_secs(3);
        while workspace.search_task.is_some() && Instant::now() < deadline {
            workspace.poll_search();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(workspace.search_task.is_none());
        assert_eq!(workspace.search_result.as_ref().unwrap().matches.len(), 1);
        assert_eq!(workspace.row_task.is_some(), needs_expansion);
        if needs_expansion {
            assert!((frame(&mut workspace).1.offset.y - before).abs() < 0.1);
            finish_rows(&mut workspace);
        }
        let target = workspace.selected.unwrap();
        let row = workspace
            .rows
            .binary_search_by_key(&target, |row| row.node)
            .unwrap();
        let row_y = row as f32 * (ROW_HEIGHT + workspace.context.style().spacing.item_spacing.y);
        let offset = frame(&mut workspace).1.offset.y;
        assert!(
            offset > before + 1000.0,
            "search did not scroll to its result"
        );
        assert!(
            (0.0..600.0).contains(&(row_y - offset)),
            "search result is outside the viewport"
        );
        assert!((frame(&mut workspace).1.offset.y - offset).abs() < 0.1);
    }
}
