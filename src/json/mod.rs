//! A lossless JSON index. Values and member names stay in the original source;
//! the tree stores only byte offsets and links, so browsing does not build a
//! second, string-heavy DOM or round numbers through floating point.

mod format;
mod parser;
mod search;

use std::{borrow::Cow, fmt, ops::Range, sync::Arc};

pub use parser::{
    MAX_DEPTH, MAX_NODES, MAX_SOURCE_BYTES, parse, parse_arc_with_cancel, parse_with_cancel,
};
pub use search::{SearchMatch, SearchOptions, SearchResult, SearchTarget};

pub type NodeId = u32;
const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Object,
    Array,
    String,
    Number,
    Bool,
    Null,
}

impl Kind {
    pub fn is_container(self) -> bool {
        matches!(self, Self::Object | Self::Array)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Object => "对象",
            Self::Array => "数组",
            Self::String => "字符串",
            Self::Number => "数字",
            Self::Bool => "布尔值",
            Self::Null => "空值",
        }
    }
}

/// Fixed-size links and offsets; no heap allocation per node.
#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    start: u32,
    end: u32,
    key_start: u32,
    key_end: u32,
    parent: u32,
    first_child: u32,
    next_sibling: u32,
    child_count: u32,
    array_index: u32,
}

impl Node {
    pub fn range(&self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    pub fn key_range(&self) -> Option<Range<usize>> {
        (self.key_start != NONE).then_some(self.key_start as usize..self.key_end as usize)
    }

    pub fn parent(&self) -> Option<NodeId> {
        (self.parent != NONE).then_some(self.parent)
    }

    pub fn first_child(&self) -> Option<NodeId> {
        (self.first_child != NONE).then_some(self.first_child)
    }

    pub fn next_sibling(&self) -> Option<NodeId> {
        (self.next_sibling != NONE).then_some(self.next_sibling)
    }

    pub fn child_count(&self) -> usize {
        self.child_count as usize
    }

    pub fn array_index(&self) -> Option<usize> {
        (self.array_index != NONE).then_some(self.array_index as usize)
    }
}

#[derive(Debug)]
pub struct Document {
    source: Arc<str>,
    nodes: Vec<Node>,
}

impl Document {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn source_arc(&self) -> Arc<str> {
        Arc::clone(&self.source)
    }

    pub fn root(&self) -> NodeId {
        0
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn raw(&self, id: NodeId) -> &str {
        &self.source[self.node(id).range()]
    }

    pub fn key(&self, id: NodeId) -> Option<Cow<'_, str>> {
        self.node(id)
            .key_range()
            .map(|range| decode_string(&self.source[range]))
    }

    pub fn scalar_text(&self, id: NodeId) -> Option<Cow<'_, str>> {
        let node = self.node(id);
        match node.kind {
            Kind::Object | Kind::Array => None,
            Kind::String => Some(decode_string(self.raw(id))),
            _ => Some(Cow::Borrowed(self.raw(id))),
        }
    }

    pub fn children(&self, id: NodeId) -> Children<'_> {
        Children {
            document: self,
            next: self.node(id).first_child(),
        }
    }

    /// RFC 6901 JSON Pointer. A duplicate object member has the same pointer
    /// as its siblings of the same name; its NodeId still identifies it exactly.
    pub fn pointer(&self, id: NodeId) -> String {
        let mut parts = Vec::new();
        let mut current = id;
        while let Some(parent) = self.node(current).parent() {
            if let Some(key) = self.key(current) {
                parts.push(key.replace('~', "~0").replace('/', "~1"));
            } else {
                parts.push(self.node(current).array_index().unwrap_or(0).to_string());
            }
            current = parent;
        }
        let mut output = String::new();
        for part in parts.into_iter().rev() {
            output.push('/');
            output.push_str(&part);
        }
        output
    }

    pub fn path(&self, id: NodeId) -> String {
        let mut parts = Vec::new();
        let mut current = id;
        while let Some(parent) = self.node(current).parent() {
            let part = if let Some(key) = self.key(current) {
                let quoted = serde_json::to_string(key.as_ref()).expect("a UTF-8 JSON key");
                format!("[{quoted}]")
            } else {
                format!("[{}]", self.node(current).array_index().unwrap_or(0))
            };
            parts.push(part);
            current = parent;
        }
        let mut output = String::from("$");
        for part in parts.into_iter().rev() {
            output.push_str(&part);
        }
        output
    }

    /// Bounded preview. In particular, it never decodes a multi-megabyte
    /// string just to paint a single tree row.
    pub fn preview(&self, id: NodeId, max_chars: usize) -> String {
        let node = self.node(id);
        match node.kind {
            Kind::Object => format!("{{ {} 个字段 }}", node.child_count()),
            Kind::Array => format!("[ {} 项 ]", node.child_count()),
            _ => truncate(self.raw(id), max_chars),
        }
    }

    pub fn index_bytes(&self) -> usize {
        self.nodes.len() * std::mem::size_of::<Node>()
    }

    pub fn allocated_index_bytes(&self) -> usize {
        self.nodes.capacity() * std::mem::size_of::<Node>()
    }
}

pub struct Children<'a> {
    document: &'a Document,
    next: Option<NodeId>,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next?;
        self.next = self.document.node(id).next_sibling();
        Some(id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    pub message: String,
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}（第 {} 行，第 {} 列）",
            self.message, self.line, self.column
        )
    }
}

impl std::error::Error for JsonError {}

fn decode_string(raw: &str) -> Cow<'_, str> {
    let content = &raw[1..raw.len() - 1];
    if content.as_bytes().contains(&b'\\') {
        Cow::Owned(serde_json::from_str(raw).expect("the parser validated string escapes"))
    } else {
        Cow::Borrowed(content)
    }
}

pub fn truncate(value: &str, max_chars: usize) -> String {
    match value.char_indices().nth(max_chars) {
        Some((end, _)) => format!("{}…", &value[..end]),
        None => value.to_owned(),
    }
}

#[cfg(test)]
mod tests;
