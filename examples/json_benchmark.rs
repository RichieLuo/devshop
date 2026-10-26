//! Reproducible, synthetic large-document benchmark; no network or user data.
use devshop::json::{SearchOptions, parse};
use std::time::Instant;

fn main() {
    let mib: usize = std::env::args()
        .nth(1)
        .and_then(|x| x.parse().ok())
        .unwrap_or(32);
    let target = mib.saturating_mul(1024 * 1024);
    let row = r#"{"id":9007199254740993,"name":"开发测试","message":"ordinary log entry","nested":{"active":true,"value":123.4567890123456789}}"#;
    let mut source = String::with_capacity(target + row.len() + 100);
    source.push('[');
    let mut rows = 0usize;
    while source.len() + row.len() + 100 < target {
        if rows != 0 {
            source.push(',');
        }
        source.push_str(row);
        rows += 1;
    }
    source.push_str(",{\"name\":\"unique-devshop-needle\"}]");
    let bytes = source.len();
    let start = Instant::now();
    let document = parse(source).expect("synthetic JSON parses");
    let parse_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let result = document
        .search("unique-devshop-needle", &SearchOptions::default())
        .unwrap();
    let search_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(result.total, 1);
    assert_eq!(document.raw(2), "9007199254740993");
    println!(
        "{{\"bytes\":{bytes},\"rows\":{rows},\"nodes\":{},\"index_bytes\":{},\"parse_ms\":{parse_ms:.2},\"search_ms\":{search_ms:.2},\"hits\":{}}}",
        document.nodes().len(),
        document.index_bytes(),
        result.total
    );
}
