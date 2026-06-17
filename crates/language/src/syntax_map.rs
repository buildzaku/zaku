#[cfg(debug_assertions)]
use std::cmp::Ordering;
use std::{
    fmt,
    ops::{ControlFlow, Deref, Range},
    sync::{Arc, LazyLock, mpsc},
    time::{Duration, Instant},
};
use sum_tree::{Dimensions, Item, SumTree, Summary};
use text::{
    Anchor, BufferId, BufferSnapshot as TextBufferSnapshot, Point, Rope, ToOffset, ToPoint,
};
use tree_sitter::{Node, ParseOptions, Tree};

#[cfg(debug_assertions)]
use crate::LanguageId;
use crate::{Grammar, Language, with_parser};

#[derive(Copy, Clone, Debug)]
pub struct ParseTimeout;

impl std::error::Error for ParseTimeout {}

impl fmt::Display for ParseTimeout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "parse timeout")
    }
}

pub trait ToTreeSitterPoint {
    fn to_ts_point(self) -> tree_sitter::Point;
    fn from_ts_point(point: tree_sitter::Point) -> Self;
}

impl ToTreeSitterPoint for Point {
    fn to_ts_point(self) -> tree_sitter::Point {
        tree_sitter::Point::new(
            usize::try_from(self.row).expect("point row should fit in usize"),
            usize::try_from(self.column).expect("point column should fit in usize"),
        )
    }

    fn from_ts_point(point: tree_sitter::Point) -> Self {
        Point::new(
            u32::try_from(point.row).expect("tree-sitter row should fit in u32"),
            u32::try_from(point.column).expect("tree-sitter column should fit in u32"),
        )
    }
}

#[derive(Clone)]
pub struct SyntaxSnapshot {
    layers: SumTree<SyntaxLayerEntry>,
    parsed_version: clock::Global,
    interpolated_version: clock::Global,
    update_count: usize,
}

impl Drop for SyntaxSnapshot {
    fn drop(&mut self) {
        static DROP_TX: LazyLock<mpsc::Sender<SumTree<SyntaxLayerEntry>>> = LazyLock::new(|| {
            let (sender, receiver) = mpsc::channel();
            std::thread::Builder::new()
                .name("SyntaxSnapshot::drop".into())
                .spawn(move || while receiver.recv().is_ok() {})
                .expect("drop thread should spawn");
            sender
        });

        let empty_layers = SumTree::from_summary(SyntaxLayerSummary {
            min_depth: 0,
            max_depth: 0,
            range: Anchor::min_min_range_for_buffer(
                BufferId::new(1).expect("buffer id should be nonzero"),
            ),
        });
        let layers = std::mem::replace(&mut self.layers, empty_layers);
        if DROP_TX.send(layers).is_err() {
            log::debug!("Failed to drop syntax snapshot on background thread");
        }
    }
}

impl SyntaxSnapshot {
    fn new(text: &TextBufferSnapshot) -> Self {
        Self {
            layers: SumTree::new(text),
            parsed_version: clock::Global::default(),
            interpolated_version: clock::Global::default(),
            update_count: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn root_language(&self) -> Option<Arc<Language>> {
        Some(self.layers.first()?.language.clone())
    }

    pub fn update_count(&self) -> usize {
        self.update_count
    }

    pub fn tree(&self) -> Option<&Tree> {
        Some(&self.layers.first()?.tree)
    }

    pub fn parsed_version(&self) -> &clock::Global {
        &self.parsed_version
    }

    pub fn interpolated_version(&self) -> &clock::Global {
        &self.interpolated_version
    }

    pub fn interpolate(&mut self, text: &TextBufferSnapshot) {
        let edits = text
            .anchored_edits_since::<Dimensions<usize, Point>>(&self.interpolated_version)
            .collect::<Vec<_>>();
        self.interpolated_version.clone_from(text.version());
        if edits.is_empty() {
            return;
        }

        if let Some(mut layer) = self.layers.first().cloned() {
            for (edit, _) in edits {
                layer.tree.edit(&tree_sitter::InputEdit {
                    start_byte: edit.new.start.0,
                    old_end_byte: edit.new.start.0 + (edit.old.end.0 - edit.old.start.0),
                    new_end_byte: edit.new.end.0,
                    start_position: edit.new.start.1.to_ts_point(),
                    old_end_position: (edit.new.start.1 + (edit.old.end.1 - edit.old.start.1))
                        .to_ts_point(),
                    new_end_position: edit.new.end.1.to_ts_point(),
                });
            }
            debug_assert!(
                layer.tree.root_node().end_byte() <= text.len(),
                "tree's size {}, is larger than text size {}",
                layer.tree.root_node().end_byte(),
                text.len(),
            );
            layer.range = Anchor::min_max_range_for_buffer(text.remote_id());
            let mut layers = SumTree::new(text);
            layers.push(layer, text);
            self.layers = layers;
        }
    }

    pub fn reparse(&mut self, text: &TextBufferSnapshot, root_language: Arc<Language>) {
        match self.reparse_inner(text, root_language, None) {
            Ok(()) => {}
            Err(ParseTimeout) => unreachable!("unbounded parse should not time out"),
        }
    }

    pub fn reparse_with_timeout(
        &mut self,
        text: &TextBufferSnapshot,
        root_language: Arc<Language>,
        budget: Duration,
    ) -> Result<(), ParseTimeout> {
        self.reparse_inner(text, root_language, Some(budget))
    }

    fn reparse_inner(
        &mut self,
        text: &TextBufferSnapshot,
        root_language: Arc<Language>,
        mut budget: Option<Duration>,
    ) -> Result<(), ParseTimeout> {
        let Some(grammar) = root_language.grammar().cloned() else {
            self.layers = SumTree::new(text);
            self.parsed_version.clone_from(text.version());
            self.interpolated_version.clone_from(text.version());
            self.update_count += 1;
            #[cfg(debug_assertions)]
            self.check_invariants(text);
            return Ok(());
        };

        let tree = parse_text(grammar.as_ref(), text.as_rope(), self.tree(), &mut budget)?;
        let mut layers = SumTree::new(text);
        layers.push(
            SyntaxLayerEntry {
                depth: 0,
                range: Anchor::min_max_range_for_buffer(text.remote_id()),
                tree,
                language: root_language,
            },
            text,
        );
        self.layers = layers;
        self.parsed_version.clone_from(text.version());
        self.interpolated_version.clone_from(text.version());
        self.update_count += 1;
        #[cfg(debug_assertions)]
        self.check_invariants(text);
        Ok(())
    }

    #[cfg(debug_assertions)]
    fn check_invariants(&self, text: &TextBufferSnapshot) {
        let mut max_depth = 0;
        let mut previous_layer: Option<(Range<Anchor>, Option<LanguageId>)> = None;
        for layer in self.layers.iter() {
            match Ord::cmp(&layer.depth, &max_depth) {
                Ordering::Less => panic!("layers out of order"),
                Ordering::Equal => {
                    if let Some((previous_range, previous_language_id)) = previous_layer {
                        match layer.range.start.cmp(&previous_range.start, text) {
                            Ordering::Less => panic!("layers out of order"),
                            Ordering::Equal => match layer.range.end.cmp(&previous_range.end, text)
                            {
                                Ordering::Less => panic!("layers out of order"),
                                Ordering::Equal => {
                                    let language_id = Some(layer.language.id);
                                    if language_id < previous_language_id {
                                        panic!("layers out of order");
                                    }
                                }
                                Ordering::Greater => {}
                            },
                            Ordering::Greater => {}
                        }
                    }
                    previous_layer = Some((layer.range.clone(), Some(layer.language.id)));
                }
                Ordering::Greater => {
                    previous_layer = None;
                }
            }

            max_depth = layer.depth;
        }
    }

    #[cfg(test)]
    pub fn layers<'a>(&'a self, buffer: &'a TextBufferSnapshot) -> Vec<SyntaxLayer<'a>> {
        self.layers_for_range(0..buffer.len(), buffer).collect()
    }

    pub fn layers_for_range<'a, T: ToOffset>(
        &'a self,
        range: Range<T>,
        buffer: &'a TextBufferSnapshot,
    ) -> impl 'a + Iterator<Item = SyntaxLayer<'a>> {
        let start_offset = range.start.to_offset(buffer);
        let end_offset = range.end.to_offset(buffer);
        let start = buffer.anchor_before(start_offset);
        let end = buffer.anchor_after(end_offset);

        let mut cursor = self.layers.filter::<_, ()>(buffer, move |summary| {
            if summary.max_depth > summary.min_depth {
                true
            } else {
                let is_before_start = summary.range.end.cmp(&start, buffer).is_lt();
                let is_after_end = summary.range.start.cmp(&end, buffer).is_gt();
                !is_before_start && !is_after_end
            }
        });

        cursor.next();
        std::iter::from_fn(move || {
            let layer = cursor.item()?;
            let syntax_layer = SyntaxLayer {
                language: &layer.language,
                tree: &layer.tree,
                offset: (
                    layer.range.start.to_offset(buffer),
                    layer.range.start.to_point(buffer).to_ts_point(),
                ),
            };
            cursor.next();
            Some(syntax_layer)
        })
    }
}

#[derive(Debug)]
pub struct SyntaxLayer<'a> {
    pub language: &'a Arc<Language>,
    tree: &'a Tree,
    pub(crate) offset: (usize, tree_sitter::Point),
}

impl<'a> SyntaxLayer<'a> {
    pub fn to_owned(&self) -> OwnedSyntaxLayer {
        OwnedSyntaxLayer {
            language: self.language.clone(),
            tree: self.tree.clone(),
            offset: self.offset,
        }
    }

    pub fn node(&self) -> Node<'a> {
        self.tree
            .root_node_with_offset(self.offset.0, self.offset.1)
    }
}

#[derive(Clone)]
pub struct OwnedSyntaxLayer {
    pub language: Arc<Language>,
    tree: Tree,
    pub offset: (usize, tree_sitter::Point),
}

impl OwnedSyntaxLayer {
    pub fn node(&self) -> Node<'_> {
        self.tree
            .root_node_with_offset(self.offset.0, self.offset.1)
    }
}

#[derive(Clone)]
struct SyntaxLayerEntry {
    depth: usize,
    range: Range<Anchor>,
    tree: Tree,
    language: Arc<Language>,
}

impl Item for SyntaxLayerEntry {
    type Summary = SyntaxLayerSummary;

    fn summary(&self, _cx: &TextBufferSnapshot) -> Self::Summary {
        SyntaxLayerSummary {
            min_depth: self.depth,
            max_depth: self.depth,
            range: self.range.clone(),
        }
    }
}

impl fmt::Debug for SyntaxLayerEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SyntaxLayer")
            .field("depth", &self.depth)
            .field("range", &self.range)
            .field("tree", &self.tree)
            .field("language", &self.language)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct SyntaxLayerSummary {
    min_depth: usize,
    max_depth: usize,
    range: Range<Anchor>,
}

impl Summary for SyntaxLayerSummary {
    type Context<'a> = &'a TextBufferSnapshot;

    fn zero(buffer: &TextBufferSnapshot) -> Self {
        Self {
            max_depth: 0,
            min_depth: 0,
            range: Anchor::max_for_buffer(buffer.remote_id())
                ..Anchor::min_for_buffer(buffer.remote_id()),
        }
    }

    fn add_summary(&mut self, other: &Self, buffer: Self::Context<'_>) {
        if other.max_depth > self.max_depth {
            self.max_depth = other.max_depth;
            self.range = other.range.clone();
        } else {
            if self.range.start.is_max() && self.range.end.is_max() {
                self.range.start = other.range.start;
            }
            if other.range.end.cmp(&self.range.end, buffer).is_gt() {
                self.range.end = other.range.end;
            }
        }
    }
}

pub struct SyntaxMap {
    snapshot: SyntaxSnapshot,
}

impl SyntaxMap {
    pub fn new(text: &TextBufferSnapshot) -> Self {
        Self {
            snapshot: SyntaxSnapshot::new(text),
        }
    }

    pub fn snapshot(&self) -> SyntaxSnapshot {
        self.snapshot.clone()
    }

    pub fn interpolate(&mut self, text: &TextBufferSnapshot) {
        self.snapshot.interpolate(text);
    }

    #[cfg(test)]
    pub fn reparse(&mut self, root_language: Arc<Language>, text: &TextBufferSnapshot) {
        self.snapshot.reparse(text, root_language);
    }

    pub fn did_parse(&mut self, snapshot: SyntaxSnapshot) {
        self.snapshot = snapshot;
    }

    pub fn clear(&mut self, text: &TextBufferSnapshot) {
        let update_count = self.snapshot.update_count + 1;
        self.snapshot = SyntaxSnapshot::new(text);
        self.snapshot.update_count = update_count;
    }
}

impl Deref for SyntaxMap {
    type Target = SyntaxSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

fn parse_text(
    grammar: &Grammar,
    text: &Rope,
    old_tree: Option<&Tree>,
    parse_budget: &mut Option<Duration>,
) -> Result<Tree, ParseTimeout> {
    with_parser(|parser| {
        let mut timed_out = false;
        let now = Instant::now();
        let mut progress_callback = (*parse_budget).map(|budget| {
            let timed_out = &mut timed_out;
            move |_: &_| {
                if now.elapsed() > budget {
                    *timed_out = true;
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            }
        });

        parser
            .set_language(&grammar.ts_language)
            .expect("incompatible grammar");
        let mut chunks = text.chunks_in_range(0..text.len());
        let parsed_tree = parser.parse_with_options(
            &mut move |offset, _| {
                chunks.seek(offset);
                chunks.next().unwrap_or("").as_bytes()
            },
            old_tree,
            progress_callback
                .as_mut()
                .map(|progress_callback| ParseOptions {
                    progress_callback: Some(progress_callback),
                }),
        );

        match parsed_tree {
            Some(tree) => {
                if let Some(parse_budget) = parse_budget {
                    *parse_budget = parse_budget.saturating_sub(now.elapsed());
                }
                Ok(tree)
            }
            None if timed_out => Err(ParseTimeout),
            None => panic!("tree-sitter parse should succeed"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use text::{Buffer, ReplicaId};

    use crate::LanguageConfig;

    fn json_lang() -> Arc<Language> {
        Arc::new(Language::new(
            LanguageConfig {
                name: "JSON".into(),
                ..Default::default()
            },
            Some(tree_sitter_json::LANGUAGE.into()),
        ))
    }

    fn range_for_text(buffer: &Buffer, text: &str) -> Range<usize> {
        let start = buffer.as_rope().to_string().find(text).unwrap();
        start..start + text.len()
    }

    #[track_caller]
    fn assert_layers_for_range(
        syntax_map: &SyntaxMap,
        buffer: &TextBufferSnapshot,
        range: Range<Point>,
        expected_layers: &[&str],
    ) {
        let layers = syntax_map
            .layers_for_range(range, buffer)
            .collect::<Vec<_>>();
        assert_eq!(
            layers.len(),
            expected_layers.len(),
            "wrong number of layers"
        );
        for (index, (layer, expected_sexp)) in layers.iter().zip(expected_layers.iter()).enumerate()
        {
            let actual_sexp = layer.node().to_sexp();
            assert_eq!(
                actual_sexp, *expected_sexp,
                "layer {index} had the wrong syntax tree"
            );
        }
    }

    #[test]
    fn test_syntax_map_layers_for_range() {
        let language = json_lang();
        let mut buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            r#"{"items":[]}"#,
        );
        let mut syntax_map = SyntaxMap::new(buffer.snapshot());
        syntax_map.reparse(language.clone(), buffer.snapshot());

        assert_layers_for_range(
            &syntax_map,
            buffer.snapshot(),
            Point::new(0, 0)..Point::new(0, 0),
            &["(document (object (pair key: (string (string_content)) value: (array))))"],
        );

        let array_range = range_for_text(&buffer, "[]");
        buffer.edit([(array_range, "{}")]);
        syntax_map.interpolate(buffer.snapshot());
        syntax_map.reparse(language.clone(), buffer.snapshot());

        assert_layers_for_range(
            &syntax_map,
            buffer.snapshot(),
            Point::new(0, 9)..Point::new(0, 11),
            &["(document (object (pair key: (string (string_content)) value: (object))))"],
        );

        assert!(buffer.undo().is_some());
        syntax_map.interpolate(buffer.snapshot());
        syntax_map.reparse(language, buffer.snapshot());

        assert_layers_for_range(
            &syntax_map,
            buffer.snapshot(),
            Point::new(0, 9)..Point::new(0, 11),
            &["(document (object (pair key: (string (string_content)) value: (array))))"],
        );
    }
}
