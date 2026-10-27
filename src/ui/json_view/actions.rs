use super::*;

impl JsonWorkspace {
    pub(super) fn launch_parse(&mut self) {
        if self.has_node_draft() {
            self.action_error = Some("请先应用节点修改，或在节点详情中取消编辑。".into());
            return;
        }
        self.cancel_work();
        self.cancel_search();
        self.parse_due = None;
        let source = Arc::clone(&self.source);
        let operation = self.operation.clone();
        let dirty = self.dirty;
        let ctx = self.context.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancelled);
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = analyze(source, operation, dirty, &worker_cancel);
            if !worker_cancel.load(Ordering::Relaxed) {
                let _ = sender.send(result);
                ctx.request_repaint();
            }
        });
        self.work = Some(WorkTask {
            receiver,
            cancelled,
            label: "正在解析…",
        });
    }

    pub(super) fn launch_format(&mut self, pretty: bool) {
        if self.has_node_draft() {
            self.action_error = Some("请先应用节点修改，或在节点详情中取消编辑。".into());
            return;
        }
        let Some(document) = self.document.clone() else {
            return;
        };
        self.cancel_work();
        let ctx = self.context.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancelled);
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let output: Arc<str> =
                match document.try_format_node_with_cancel(0, pretty, &worker_cancel) {
                    Ok(output) => Arc::from(output),
                    Err(error) => {
                        if !worker_cancel.load(Ordering::Relaxed) {
                            let _ = sender.send(WorkResult::Rejected(error.to_string()));
                            ctx.request_repaint();
                        }
                        return;
                    }
                };
            if worker_cancel.load(Ordering::Relaxed) {
                return;
            }
            let result = analyze(
                output,
                Some(if pretty { "格式化" } else { "压缩" }.into()),
                true,
                &worker_cancel,
            );
            if !worker_cancel.load(Ordering::Relaxed) {
                let _ = sender.send(result);
                ctx.request_repaint();
            }
        });
        self.work = Some(WorkTask {
            receiver,
            cancelled,
            label: if pretty {
                "正在格式化…"
            } else {
                "正在压缩…"
            },
        });
    }

    pub(super) fn launch_replace(&mut self, id: NodeId, replacement: String) {
        let Some(document) = self.document.clone() else {
            return;
        };
        self.cancel_work();
        let ctx = self.context.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancelled);
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let replacement: Arc<str> = Arc::from(replacement);
            if let Err(error) =
                json::parse_arc_with_cancel(Arc::clone(&replacement), &worker_cancel)
            {
                if !worker_cancel.load(Ordering::Relaxed) {
                    let _ = sender.send(WorkResult::Rejected(format!(
                        "节点内容无效：{error}。原文未修改。"
                    )));
                    ctx.request_repaint();
                }
                return;
            }
            let range = document.node(id).range();
            let new_len = document.source().len() - range.len() + replacement.len();
            if new_len > json::MAX_SOURCE_BYTES {
                let _ = sender.send(WorkResult::Rejected(
                    "修改后超过 512 MiB，原文未修改".into(),
                ));
                ctx.request_repaint();
                return;
            }
            let mut output = String::with_capacity(new_len);
            output.push_str(&document.source()[..range.start]);
            output.push_str(&replacement);
            output.push_str(&document.source()[range.end..]);
            let result = analyze(
                Arc::from(output),
                Some("修改节点".into()),
                true,
                &worker_cancel,
            );
            if !worker_cancel.load(Ordering::Relaxed) {
                let _ = sender.send(result);
                ctx.request_repaint();
            }
        });
        self.work = Some(WorkTask {
            receiver,
            cancelled,
            label: "正在更新节点…",
        });
    }

    pub(super) fn source_edited(&mut self) {
        self.cancel_work();
        self.cancel_rows();
        self.cancel_search();
        self.source = if self.edit_text.len() > EDIT_LIMIT {
            Arc::from(std::mem::take(&mut self.edit_text))
        } else {
            Arc::from(self.edit_text.as_str())
        };
        self.lines = Arc::new(vec![0]);
        self.document = None;
        self.rows.clear();
        self.selected = None;
        self.error = None;
        self.action_error = None;
        self.dirty = true;
        self.operation = Some("编辑".into());
        self.parse_due = Some(Instant::now() + Duration::from_millis(550));
        self.context
            .request_repaint_after(Duration::from_millis(550));
    }
}

fn analyze(
    source: Arc<str>,
    operation: Option<String>,
    dirty: bool,
    cancelled: &AtomicBool,
) -> WorkResult {
    let started = Instant::now();
    let document = json::parse_arc_with_cancel(Arc::clone(&source), cancelled);
    let mut lines = vec![0];
    if source.len() <= json::MAX_SOURCE_BYTES && !cancelled.load(Ordering::Relaxed) {
        for (position, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                if lines.len() >= 2_000_000 {
                    break;
                }
                lines.push(position + 1);
            }
            if position & 0xffff == 0 && cancelled.load(Ordering::Relaxed) {
                break;
            }
        }
    }
    WorkResult::Analysis {
        source,
        document,
        lines,
        operation,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        dirty,
    }
}
