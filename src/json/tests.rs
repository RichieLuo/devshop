use std::sync::atomic::AtomicBool;

use super::*;

#[test]
fn preserves_numbers_order_duplicates_and_escapes() {
    let input = r#"{ "z":90071992547409931234567890,"a":1.230000e+400,"z":-0,"s":"\u4e2d\n" }"#;
    let document = parse(input.into()).unwrap();
    assert_eq!(
        document.minify(),
        r#"{"z":90071992547409931234567890,"a":1.230000e+400,"z":-0,"s":"\u4e2d\n"}"#
    );
    let children: Vec<_> = document.children(0).collect();
    assert_eq!(children.len(), 4);
    assert_eq!(document.raw(children[0]), "90071992547409931234567890");
    assert_eq!(document.scalar_text(children[3]).unwrap(), "中\n");
    assert_eq!(document.pointer(children[0]), document.pointer(children[2]));
    assert_ne!(children[0], children[2]);
    let pretty = document.pretty();
    assert!(pretty.contains("\n  \"z\": 90071992547409931234567890,"));
    assert_eq!(parse(pretty).unwrap().minify(), document.minify());
}

#[test]
fn pointers_use_decoded_keys_and_array_indices() {
    let document = parse(r#"{"a/b~c":[{"\u4e2d文":"\ud83d\ude00"}]}"#.into()).unwrap();
    let id = (document.nodes().len() - 1) as NodeId;
    assert_eq!(document.pointer(id), "/a~1b~0c/0/中文");
    assert_eq!(document.path(id), "$[\"a/b~c\"][0][\"中文\"]");
    assert_eq!(document.scalar_text(id).unwrap(), "😀");
}

#[test]
fn strict_validation_rejects_invalid_json() {
    for invalid in [
        "",
        " ",
        "01",
        "-",
        "1.",
        "1e",
        "1e+",
        "+1",
        ".1",
        "NaN",
        "Infinity",
        "[1,]",
        "{\"a\":1,}",
        "{a:1}",
        "[true false]",
        "{}[]",
        "true1",
        "\"line\ntext\"",
        "\"\\x00\"",
        "\"\\uD800\"",
        "\"\\uDC00\"",
        "\"\\uD800\\u0041\"",
        "\"\\uXX00\"",
        "\"unfinished",
        "/*comment*/{}",
    ] {
        assert!(parse(invalid.into()).is_err(), "accepted {invalid:?}");
    }
}

#[test]
fn valid_scalars_empty_containers_and_whitespace() {
    for valid in [
        "true",
        "false",
        "null",
        "0",
        "-0",
        "1.2e-3",
        "\"中文\"",
        "{}",
        "[]",
        " \n [ {}, [], 0 ]\r\t",
    ] {
        assert!(parse(valid.into()).is_ok(), "rejected {valid:?}");
    }
    let document = parse("{\"a\": [],\"b\":{}}".into()).unwrap();
    assert_eq!(document.pretty(), "{\n  \"a\": [],\n  \"b\": {}\n}");
}

#[test]
fn errors_report_line_column_and_byte_offset() {
    let error = parse("{\n  \"中文\": nope\n}".into()).unwrap_err();
    assert_eq!(error.line, 2);
    assert_eq!(error.column, 9);
    assert_eq!(error.offset, 14);
}

#[test]
fn search_covers_unexpanded_nodes_and_decodes_escapes() {
    let document =
        parse(r#"{"users":[{"name":"Alice"},{"na\u006de":"ALICE"},{"other":"中文"}]}"#.into())
            .unwrap();
    let values = document.search("alice", &SearchOptions::default()).unwrap();
    assert_eq!(values.total, 2);
    assert!(values.matches.iter().all(|m| m.in_value && !m.in_key));
    let keys = document
        .search(
            "name",
            &SearchOptions {
                target: SearchTarget::Keys,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(keys.total, 2);
    let exact = document
        .search(
            "Alice",
            &SearchOptions {
                case_sensitive: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(exact.total, 1);
    let regex = document
        .search(
            "^A.*e$",
            &SearchOptions {
                regex: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(regex.total, 2);
    assert_eq!(
        document
            .search("中文", &SearchOptions::default())
            .unwrap()
            .total,
        1
    );
}

#[test]
fn limits_search_results_but_counts_all_matches() {
    let document = parse("[1,1,1,1]".into()).unwrap();
    let result = document
        .search(
            "1",
            &SearchOptions {
                max_results: 2,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result.total, 4);
    assert_eq!(result.matches.len(), 2);
    assert!(result.truncated);
    assert!(!result.cancelled);
    assert!(
        document
            .search(
                "[",
                &SearchOptions {
                    regex: true,
                    ..Default::default()
                }
            )
            .is_err()
    );
}

#[test]
fn cancellation_and_depth_limits_are_explicit() {
    assert!(
        parse_with_cancel("[]".into(), &AtomicBool::new(true))
            .unwrap_err()
            .message
            .contains("取消")
    );
    let too_deep = format!(
        "{}0{}",
        "[".repeat(MAX_DEPTH + 2),
        "]".repeat(MAX_DEPTH + 2)
    );
    assert!(parse(too_deep).unwrap_err().message.contains("256"));
    let document = parse("[1,2,3]".into()).unwrap();
    assert!(
        document
            .search_with_cancel("1", &SearchOptions::default(), &AtomicBool::new(true))
            .unwrap()
            .cancelled
    );
}

#[test]
fn index_is_compact_and_preview_is_bounded() {
    assert!(std::mem::size_of::<Node>() <= 40);
    let input = format!("[\"{}\"]", "中文".repeat(100_000));
    let document = parse(input).unwrap();
    assert!(document.preview(1, 10).chars().count() <= 11);
    assert_eq!(document.index_bytes(), 2 * std::mem::size_of::<Node>());
}

#[test]
fn many_nodes_have_linear_links_and_can_be_searched() {
    let mut source = String::from("[");
    for index in 0..50_000 {
        if index > 0 {
            source.push(',');
        }
        source.push_str(&format!("{{\"id\":{index},\"status\":\"ok\"}}"));
    }
    source.push(']');
    let document = parse(source).unwrap();
    assert_eq!(document.nodes().len(), 150_001);
    assert_eq!(document.children(0).count(), 50_000);
    assert_eq!(
        document
            .search("49999", &SearchOptions::default())
            .unwrap()
            .total,
        1
    );
    let last = *document.children(0).collect::<Vec<_>>().last().unwrap();
    assert_eq!(document.node(last).array_index(), Some(49_999));
    assert_eq!(document.pointer(last), "/49999");
}

#[test]
fn streaming_format_matches_string_format_and_propagates_io_failure() {
    let document = parse("{\"a\": [1,2,3]}".into()).unwrap();
    let mut output = Vec::new();
    document.write_node(0, true, &mut output).unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), document.pretty());
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(document.write_node(0, false, &mut Broken).is_err());
}

#[test]
fn formatting_preserves_string_punctuation_and_literal_search_is_literal() {
    let raw = r#"{"s":"a, b: { [ ] } \" \\ /","a.b":"needle","aXb":"other"}"#;
    let document = parse(raw.into()).unwrap();
    assert_eq!(document.minify(), raw);
    let reparsed = parse(document.pretty()).unwrap();
    assert_eq!(reparsed.minify(), raw);
    let result = document.search("a.b", &SearchOptions::default()).unwrap();
    assert_eq!(result.total, 1);
    let regex = document
        .search(
            "a.b",
            &SearchOptions {
                regex: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(regex.total, 2);
}

#[test]
fn generated_json_round_trips_and_node_spans_stay_inside_their_parents() {
    // Different shapes, strings, escapes, empty containers and scalar types;
    // serde_json is an independent formatter/decoder for these finite fixtures.
    for index in 0..200 {
        let value = serde_json::json!({
            "sequence": index,
            "nested": [{"text": format!("中\\文\n\"{index}"), "flag": index % 2 == 0}, [], {}],
            "fraction": index as f64 / 7.0,
            "nothing": null
        });
        let source = serde_json::to_string_pretty(&value).unwrap();
        let document = parse(source).unwrap();
        let round_trip: serde_json::Value = serde_json::from_str(&document.minify()).unwrap();
        assert_eq!(round_trip, value);
        for (id, node) in document.nodes().iter().enumerate() {
            if let Some(parent) = node.parent() {
                let parent_range = document.node(parent).range();
                let range = node.range();
                assert!(range.start > parent_range.start && range.end < parent_range.end);
                assert!(document.children(parent).any(|child| child == id as NodeId));
            }
        }
    }
}

#[test]
fn interactive_formatting_respects_cancellation() {
    let document = parse("{\"a\":[1,2,3]}".into()).unwrap();
    assert!(
        document
            .try_format_node_with_cancel(0, true, &AtomicBool::new(true))
            .is_err()
    );
    assert_eq!(
        document.try_format_node(0, true).unwrap(),
        document.pretty()
    );
}

#[test]
#[ignore = "Explicit large-document measurement; run with --ignored --nocapture"]
fn large_document_measurement() {
    use std::time::Instant;
    let count: usize = std::env::var("DEVSHOP_JSON_BENCH_ROWS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(200_000);
    let padding = "x".repeat(300);
    let mut source = String::with_capacity(count * 350);
    source.push('[');
    for index in 0..count {
        if index > 0 {
            source.push(',');
        }
        source.push_str(&format!(
            "{{\"id\":{index},\"message\":\"{padding}{}\",\"ok\":true}}",
            if index + 1 == count {
                "needle-final"
            } else {
                ""
            }
        ));
    }
    source.push(']');
    let source_bytes = source.len();
    let started = Instant::now();
    let document = parse(source).unwrap();
    let parse_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let result = document
        .search("needle-final", &SearchOptions::default())
        .unwrap();
    let search_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(result.total, 1);
    assert_eq!(document.nodes().len(), count * 4 + 1);
    let started = Instant::now();
    let minified = document.try_format_node(0, false).unwrap();
    let minify_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(minified.as_str(), document.source());
    println!(
        "JSON_BENCH source_bytes={source_bytes} nodes={} index_bytes={} allocated_index_bytes={} parse_ms={parse_ms:.2} search_ms={search_ms:.2} minify_ms={minify_ms:.2}",
        document.nodes().len(),
        document.index_bytes(),
        document.allocated_index_bytes()
    );
}
