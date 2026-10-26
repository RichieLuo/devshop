use std::sync::atomic::{AtomicBool, Ordering};

use regex::RegexBuilder;

use super::{Document, NodeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SearchTarget {
    Keys,
    Values,
    #[default]
    Both,
}

#[derive(Clone, Debug)]
pub struct SearchOptions {
    pub target: SearchTarget,
    pub case_sensitive: bool,
    pub regex: bool,
    pub max_results: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            target: SearchTarget::Both,
            case_sensitive: false,
            regex: false,
            max_results: 10_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    pub node: NodeId,
    pub in_key: bool,
    pub in_value: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub total: usize,
    pub truncated: bool,
    pub cancelled: bool,
}

impl Document {
    pub fn search(&self, query: &str, options: &SearchOptions) -> Result<SearchResult, String> {
        self.search_with_cancel(query, options, &AtomicBool::new(false))
    }

    pub fn search_with_cancel(
        &self,
        query: &str,
        options: &SearchOptions,
        cancelled: &AtomicBool,
    ) -> Result<SearchResult, String> {
        let mut result = SearchResult::default();
        if query.is_empty() {
            return Ok(result);
        }
        if query.len() > 16_384 {
            return Err("搜索表达式超过 16 KiB".into());
        }
        let pattern = if options.regex {
            query.to_owned()
        } else {
            regex::escape(query)
        };
        let matcher = RegexBuilder::new(&pattern)
            .case_insensitive(!options.case_sensitive)
            .size_limit(10 * 1024 * 1024)
            .dfa_size_limit(10 * 1024 * 1024)
            .build()
            .map_err(|error| format!("搜索表达式无效：{error}"))?;
        for (index, _node) in self.nodes().iter().enumerate() {
            if cancelled.load(Ordering::Relaxed) {
                result.cancelled = true;
                break;
            }
            let id = index as NodeId;
            let in_key = options.target != SearchTarget::Values
                && self.key(id).is_some_and(|key| matcher.is_match(&key));
            let in_value = options.target != SearchTarget::Keys
                && self
                    .scalar_text(id)
                    .is_some_and(|value| matcher.is_match(&value));
            if in_key || in_value {
                result.total += 1;
                if result.matches.len() < options.max_results.min(100_000) {
                    result.matches.push(SearchMatch {
                        node: id,
                        in_key,
                        in_value,
                    });
                } else {
                    result.truncated = true;
                }
            }
        }
        Ok(result)
    }
}
