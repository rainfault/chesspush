//! Shared, versioned editable chess documents. Repertoire is a role, not another PGN model.
//! Observed games and engine evidence remain independent: this module never writes them.
use crate::{
    chess_domain::{INITIAL_FEN, OccurrenceRef, position_key},
    domain::AppError,
};
use pgn_reader::{Nag, RawComment, RawTag, Reader, SanPlus, Skip, Visitor};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Position, fen::Fen, uci::UciMove};
use std::{
    collections::{BTreeMap, HashSet},
    ops::ControlFlow,
};

#[cfg(feature = "desktop")]
pub mod desktop;
pub mod fragments;

const MAX_NODES: usize = 50_000;
const MAX_DEPTH: usize = 512;
const MAX_CHAPTERS: usize = 256;
fn error(message: impl ToString) -> AppError {
    AppError::new("knowledge", message)
}
fn new_id(prefix: &str) -> Result<String, AppError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(error)?;
    Ok(format!(
        "{prefix}-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
fn now_ms() -> i64 {
    crate::statistics::store::now_ms()
}
fn default_kind() -> String {
    "repertoire".into()
}
fn default_side() -> String {
    "white".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shape {
    pub brush: String,
    pub orig: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dest: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LinkTarget {
    Position {
        position_key: String,
    },
    Occurrence {
        #[serde(flatten)]
        occurrence: OccurrenceRef,
    },
    DocumentNode {
        document_id: String,
        revision: i64,
        chapter_id: String,
        node_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeLink {
    pub target: LinkTarget,
    pub provenance: String,
    #[serde(default)]
    pub path_uci: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_fen: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariationNode {
    pub id: String,
    pub fen: String,
    pub position_key: String,
    pub ply: u32,
    pub uci: Option<String>,
    pub san: Option<String>,
    pub comments: Vec<String>,
    #[serde(default)]
    pub starting_comments: Vec<String>,
    pub nags: Vec<u8>,
    pub shapes: Vec<Shape>,
    pub children: Vec<VariationNode>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chapter {
    pub id: String,
    pub title: String,
    pub initial_fen: String,
    pub headers: BTreeMap<String, String>,
    pub root: VariationNode,
    pub source_pgn: Option<String>,
    #[serde(default)]
    pub links: Vec<KnowledgeLink>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KnowledgeDocument {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub side: String,
    pub revision: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub metadata: BTreeMap<String, Value>,
    pub chapters: Vec<Chapter>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSummary {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub side: String,
    pub revision: i64,
    pub chapters: usize,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub attachment: bool,
    #[serde(default)]
    pub attachment_active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default)]
    pub openings: Vec<fragments::FragmentOpening>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRequest {
    pub title: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default = "default_side")]
    pub side: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_fen: Option<String>,
    #[serde(default)]
    pub links: Vec<KnowledgeLink>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportRequest {
    pub title: String,
    #[serde(default = "default_side")]
    pub side: String,
    pub pgn: String,
    #[serde(default)]
    pub document_id: Option<String>,
    #[serde(default)]
    pub expected_revision: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditOperation {
    AddMove {
        chapter_id: String,
        parent_id: String,
        uci: String,
    },
    RemoveVariation {
        chapter_id: String,
        node_id: String,
    },
    PromoteVariation {
        chapter_id: String,
        node_id: String,
    },
    Annotate {
        chapter_id: String,
        node_id: String,
        comments: Vec<String>,
        nags: Vec<u8>,
        shapes: Vec<Shape>,
    },
    Rename {
        title: String,
        side: String,
    },
    AddChapter {
        title: String,
        #[serde(default)]
        initial_fen: Option<String>,
        #[serde(default)]
        links: Vec<KnowledgeLink>,
    },
    RenameChapter {
        chapter_id: String,
        title: String,
    },
    RemoveChapter {
        chapter_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalMove {
    pub uci: String,
    pub san: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Navigation {
    pub document_id: String,
    pub revision: i64,
    pub chapter_id: String,
    pub node_id: String,
    pub path_node_ids: Vec<String>,
    pub path_uci: Vec<String>,
    pub initial_fen: String,
    pub fen: String,
    pub position_key: String,
    pub turn: String,
    pub legal_moves: Vec<LegalMove>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedMove {
    pub document_id: String,
    pub document_revision: i64,
    pub chapter_id: String,
    pub node_id: String,
    pub parent_node_id: String,
    pub from_key: String,
    pub to_key: String,
    pub fen: String,
    pub uci: String,
    pub san: String,
    pub side: String,
    pub initial_fen: String,
    pub path_uci: Vec<String>,
    pub links: Vec<KnowledgeLink>,
}
fn position(fen: &str) -> Result<Chess, AppError> {
    fen.parse::<Fen>()
        .map_err(error)?
        .into_position(CastlingMode::Standard)
        .map_err(error)
}
fn node(
    pos: &Chess,
    ply: u32,
    uci: Option<String>,
    san: Option<String>,
) -> Result<VariationNode, AppError> {
    Ok(VariationNode {
        id: new_id("node")?,
        fen: Fen::from_position(pos, EnPassantMode::Always).to_string(),
        position_key: position_key(pos),
        ply,
        uci,
        san,
        comments: vec![],
        starting_comments: vec![],
        nags: vec![],
        shapes: vec![],
        children: vec![],
    })
}
fn chapter(
    title: String,
    fen: Option<&str>,
    links: Vec<KnowledgeLink>,
) -> Result<Chapter, AppError> {
    let pos = position(fen.unwrap_or(INITIAL_FEN))?;
    let root = node(&pos, 0, None, None)?;
    Ok(Chapter {
        id: new_id("chapter")?,
        title,
        initial_fen: root.fen.clone(),
        headers: BTreeMap::new(),
        root,
        source_pgn: None,
        links,
    })
}
fn at<'a>(root: &'a VariationNode, path: &[usize]) -> &'a VariationNode {
    let mut current = root;
    for &i in path {
        current = &current.children[i];
    }
    current
}
fn at_mut<'a>(root: &'a mut VariationNode, path: &[usize]) -> &'a mut VariationNode {
    let mut current = root;
    for &i in path {
        current = &mut current.children[i];
    }
    current
}
fn find_path(node: &VariationNode, id: &str, path: &mut Vec<usize>) -> bool {
    if node.id == id {
        return true;
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        if find_path(child, id, path) {
            return true;
        }
        path.pop();
    }
    false
}
fn node_path(root: &VariationNode, id: &str) -> Result<Vec<usize>, AppError> {
    let mut path = vec![];
    if find_path(root, id, &mut path) {
        Ok(path)
    } else {
        Err(AppError::new("not_found", "Узел документа не найден"))
    }
}

struct Parsing {
    chapter: Chapter,
    path: Vec<usize>,
    variations: Vec<Vec<usize>>,
    nodes: usize,
    partial_comment: String,
    variation_start: bool,
    starting_comments: Vec<String>,
    mainline_depth: BTreeMap<String, usize>,
}
struct TreeVisitor {
    moves: usize,
}
impl Visitor for TreeVisitor {
    type Tags = BTreeMap<String, String>;
    type Movetext = Parsing;
    type Output = Result<Chapter, AppError>;
    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(BTreeMap::new())
    }
    fn tag(
        &mut self,
        tags: &mut Self::Tags,
        name: &[u8],
        value: RawTag<'_>,
    ) -> ControlFlow<Self::Output> {
        let key = String::from_utf8_lossy(name).into_owned();
        if tags.contains_key(&key) {
            return ControlFlow::Break(Err(error(format!("Повторяющийся заголовок PGN: {key}"))));
        }
        tags.insert(key, value.decode_utf8_lossy().into());
        ControlFlow::Continue(())
    }
    fn begin_movetext(&mut self, tags: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        if tags.get("Variant").is_some_and(|v| {
            !["Standard", "standard", "From Position", "fromPosition"].contains(&v.as_str())
        }) {
            return ControlFlow::Break(Err(error("Поддерживаются только стандартные шахматы")));
        }
        let title = tags
            .get("ChapterName")
            .or_else(|| tags.get("Event"))
            .cloned()
            .unwrap_or_else(|| "Глава".into());
        let mut c = match chapter(title, tags.get("FEN").map(String::as_str), vec![]) {
            Ok(c) => c,
            Err(e) => return ControlFlow::Break(Err(e)),
        };
        c.headers = tags;
        ControlFlow::Continue(Parsing {
            chapter: c,
            path: vec![],
            variations: vec![],
            nodes: 1,
            partial_comment: String::new(),
            variation_start: false,
            starting_comments: vec![],
            mainline_depth: BTreeMap::new(),
        })
    }
    fn san(&mut self, line: &mut Parsing, san: SanPlus) -> ControlFlow<Self::Output> {
        self.moves += 1;
        if line.nodes >= MAX_NODES || line.path.len() >= MAX_DEPTH {
            return ControlFlow::Break(Err(error("Превышен размер дерева PGN")));
        }
        let parent = at(&line.chapter.root, &line.path);
        let mut pos = match position(&parent.fen) {
            Ok(p) => p,
            Err(e) => return ControlFlow::Break(Err(e)),
        };
        let mv = match san.san.to_move(&pos) {
            Ok(m) => m,
            Err(e) => return ControlFlow::Break(Err(error(format!("Нелегальный ход {san}: {e}")))),
        };
        let uci = UciMove::from_move(mv, CastlingMode::Standard).to_string();
        let normalized = SanPlus::from_move(pos.clone(), mv).to_string();
        pos.play_unchecked(mv);
        let existing = parent
            .children
            .iter()
            .position(|n| n.uci.as_deref() == Some(&uci));
        let parent_id = parent.id.clone();
        let priority = line.variations.len();
        let promote = line
            .mainline_depth
            .get(&parent_id)
            .is_some_and(|previous| priority < *previous);
        line.mainline_depth
            .entry(parent_id)
            .and_modify(|previous| *previous = (*previous).min(priority))
            .or_insert(priority);
        let mut index = if let Some(index) = existing {
            index
        } else {
            let n = match node(&pos, parent.ply + 1, Some(uci), Some(normalized)) {
                Ok(n) => n,
                Err(e) => return ControlFlow::Break(Err(e)),
            };
            let parent = at_mut(&mut line.chapter.root, &line.path);
            parent.children.push(n);
            line.nodes += 1;
            parent.children.len() - 1
        };
        if promote && index > 0 {
            let parent = at_mut(&mut line.chapter.root, &line.path);
            let node = parent.children.remove(index);
            parent.children.insert(0, node);
            for paused in &mut line.variations {
                if paused.starts_with(&line.path) && paused.len() > line.path.len() {
                    let child = &mut paused[line.path.len()];
                    if *child == index {
                        *child = 0;
                    } else if *child < index {
                        *child += 1;
                    }
                }
            }
            index = 0;
        }
        line.path.push(index);
        if line.variation_start {
            at_mut(&mut line.chapter.root, &line.path)
                .starting_comments
                .extend(std::mem::take(&mut line.starting_comments));
            line.variation_start = false;
        }
        ControlFlow::Continue(())
    }
    fn nag(&mut self, line: &mut Parsing, nag: Nag) -> ControlFlow<Self::Output> {
        let n = at_mut(&mut line.chapter.root, &line.path);
        if !n.nags.contains(&nag.0) {
            n.nags.push(nag.0);
        }
        ControlFlow::Continue(())
    }
    fn partial_comment(
        &mut self,
        line: &mut Parsing,
        comment: RawComment<'_>,
    ) -> ControlFlow<Self::Output> {
        line.partial_comment
            .push_str(&String::from_utf8_lossy(comment.0));
        ControlFlow::Continue(())
    }
    fn comment(
        &mut self,
        line: &mut Parsing,
        comment: RawComment<'_>,
    ) -> ControlFlow<Self::Output> {
        let text = format!(
            "{}{}",
            std::mem::take(&mut line.partial_comment),
            String::from_utf8_lossy(comment.0)
        );
        if line.variation_start {
            line.starting_comments.push(text);
            return ControlFlow::Continue(());
        }
        let n = at_mut(&mut line.chapter.root, &line.path);
        let (text, shapes) = parse_shapes(&text);
        if !text.is_empty() {
            n.comments.push(text);
        }
        n.shapes.extend(shapes);
        ControlFlow::Continue(())
    }
    fn begin_variation(&mut self, line: &mut Parsing) -> ControlFlow<Self::Output, Skip> {
        if line.path.is_empty() || line.variations.len() >= MAX_DEPTH {
            return ControlFlow::Break(Err(error("Вариант PGN не имеет родительского хода")));
        }
        line.variations.push(line.path.clone());
        line.path.pop();
        line.variation_start = true;
        ControlFlow::Continue(Skip(false))
    }
    fn end_variation(&mut self, line: &mut Parsing) -> ControlFlow<Self::Output> {
        if line.variation_start {
            return ControlFlow::Break(Err(error("Пустой PGN-вариант")));
        }
        match line.variations.pop() {
            Some(path) => line.path = path,
            None => return ControlFlow::Break(Err(error("Лишняя закрывающая скобка PGN"))),
        }
        ControlFlow::Continue(())
    }
    fn outcome(
        &mut self,
        line: &mut Parsing,
        outcome: shakmaty::Outcome,
    ) -> ControlFlow<Self::Output> {
        if line.variations.is_empty() {
            let result = outcome.to_string();
            if line
                .chapter
                .headers
                .get("Result")
                .is_some_and(|r| r != "*" && result != "*" && r != &result)
            {
                return ControlFlow::Break(Err(error(
                    "Результаты заголовка и записи PGN не совпадают",
                )));
            }
            if result != "*" || !line.chapter.headers.contains_key("Result") {
                line.chapter.headers.insert("Result".into(), result);
            }
        }
        ControlFlow::Continue(())
    }
    fn end_game(&mut self, line: Parsing) -> Self::Output {
        if !line.variations.is_empty() {
            return Err(error("Незакрытый вариант PGN"));
        }
        Ok(line.chapter)
    }
}
fn square(s: &str) -> bool {
    s.len() == 2
        && s.as_bytes()[0] >= b'a'
        && s.as_bytes()[0] <= b'h'
        && s.as_bytes()[1] >= b'1'
        && s.as_bytes()[1] <= b'8'
}
fn parse_shapes(text: &str) -> (String, Vec<Shape>) {
    let re = regex::Regex::new(r"\[%(cal|csl)\s+([^\]]*)\]").unwrap();
    let mut shapes = vec![];
    let cleaned = re.replace_all(text, |caps: &regex::Captures<'_>| {
        let mut parsed = vec![];
        for item in caps[2].split(',').map(str::trim) {
            let expected = if &caps[1] == "cal" { 5 } else { 3 };
            if item.len() != expected || !item.is_ascii() {
                return caps[0].to_owned();
            }
            let brush = match &item[..1] {
                "R" => "red",
                "G" => "green",
                "B" => "blue",
                "Y" => "yellow",
                _ => return caps[0].to_owned(),
            };
            if !square(&item[1..3]) || (expected == 5 && !square(&item[3..5])) {
                return caps[0].to_owned();
            }
            parsed.push(Shape {
                brush: brush.into(),
                orig: item[1..3].into(),
                dest: (expected == 5).then(|| item[3..5].into()),
            });
        }
        shapes.extend(parsed);
        String::new()
    });
    (cleaned.trim().to_owned(), shapes)
}

// pgn-reader is deliberately tolerant. Reject unknown tokens/unbalanced delimiters rather
// than silently accepting a truncated or flattened document. Raw input is also retained.
fn normalize_line_comments(pgn: &str) -> Result<String, AppError> {
    let mut out = String::with_capacity(pgn.len());
    let mut chars = pgn.chars().peekable();
    let (mut braced, mut tagged, mut quoted, mut escaped) = (false, false, false, false);
    while let Some(c) = chars.next() {
        if braced {
            out.push(c);
            if c == '}' {
                braced = false;
            }
            continue;
        }
        if tagged {
            out.push(c);
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' && quoted {
                escaped = true;
                continue;
            }
            if c == '"' {
                quoted = !quoted;
            }
            if c == ']' && !quoted {
                tagged = false;
            }
            continue;
        }
        match c {
            '{' => {
                braced = true;
                out.push(c);
            }
            '[' => {
                tagged = true;
                out.push(c);
            }
            ';' => {
                let mut text = String::new();
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        break;
                    }
                    text.push(ch);
                }
                if text.contains(['{', '}']) {
                    return Err(error(
                        "Скобки внутри строкового PGN-комментария не поддерживаются",
                    ));
                }
                out.push('{');
                out.push_str(text.trim_end_matches('\r'));
                out.push_str("}\n");
            }
            _ => out.push(c),
        }
    }
    Ok(out)
}
fn validate_pgn(pgn: &str) -> Result<usize, AppError> {
    if pgn.trim().is_empty() || pgn.len() > 10 * 1024 * 1024 {
        return Err(error("Пустой или слишком большой PGN"));
    }
    let mut clean = String::new();
    let header_re =
        regex::Regex::new(r#"^\s*[A-Za-z][A-Za-z0-9_:-]*\s+"(?:[^"\\]|\\.)*"\s*$"#).unwrap();
    let mut chars = pgn.chars().peekable();
    let mut depth = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let mut closed = false;
                for ch in chars.by_ref() {
                    if ch == '}' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err(error("Незакрытый комментарий PGN"));
                }
                clean.push(' ');
            }
            ';' => {
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        break;
                    }
                }
                clean.push(' ');
            }
            '[' => {
                let mut quote = false;
                let mut escape = false;
                let mut closed = false;
                let mut content = String::new();
                for ch in chars.by_ref() {
                    content.push(ch);
                    if escape {
                        escape = false;
                        continue;
                    }
                    if ch == '\\' && quote {
                        escape = true;
                        continue;
                    }
                    if ch == '"' {
                        quote = !quote;
                    }
                    if ch == ']' && !quote {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err(error("Незакрытый заголовок PGN"));
                }
                content.pop();
                if !header_re.is_match(&content) {
                    return Err(error("Некорректный заголовок PGN"));
                }
                clean.push(' ');
            }
            '(' => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(error("Слишком глубокий PGN"));
                }
                clean.push(' ');
            }
            ')' => {
                if depth == 0 {
                    return Err(error("Лишняя скобка PGN"));
                }
                depth -= 1;
                clean.push(' ');
            }
            '}' => return Err(error("Лишняя скобка комментария PGN")),
            _ => clean.push(c),
        }
    }
    if depth != 0 {
        return Err(error("Незакрытый вариант PGN"));
    }
    let clean = clean.replace('$', " $");
    let mut moves = 0;
    for raw in clean.split_whitespace() {
        let mut token = raw.trim_start_matches('\u{feff}');
        if ["*", "1-0", "0-1", "1/2-1/2"].contains(&token) {
            continue;
        }
        if token.starts_with('$') && token[1..].parse::<u8>().is_ok() {
            continue;
        }
        if let Some(dot) = token.find('.')
            && token[..dot].chars().all(|c| c.is_ascii_digit())
        {
            token = token[dot..].trim_start_matches('.');
        }
        if token.is_empty() {
            continue;
        }
        token = token.trim_end_matches(['!', '?']);
        if token.is_empty() {
            continue;
        }
        let normalized_castle = token.starts_with("0-0").then(|| token.replace('0', "O"));
        if SanPlus::from_ascii(normalized_castle.as_deref().unwrap_or(token).as_bytes()).is_err() {
            return Err(error(format!("Неподдерживаемый токен PGN: {raw}")));
        }
        moves += 1;
    }
    Ok(moves)
}
pub fn parse_pgn(pgn: &str) -> Result<Vec<Chapter>, AppError> {
    let expected_moves = validate_pgn(pgn)?;
    let normalized = normalize_line_comments(pgn)?;
    let mut reader = Reader::new(normalized.as_bytes());
    let mut visitor = TreeVisitor { moves: 0 };
    let mut chapters = vec![];
    while let Some(parsed) = reader.read_game(&mut visitor).map_err(error)? {
        let mut chapter = parsed?;
        if chapters.is_empty() {
            chapter.source_pgn = Some(pgn.to_owned());
        }
        chapters.push(chapter);
        if chapters.len() > MAX_CHAPTERS {
            return Err(error("Слишком много глав PGN"));
        }
    }
    if chapters.is_empty() {
        return Err(error("PGN не содержит глав"));
    }
    if visitor.moves != expected_moves {
        return Err(error("PGN содержит ходы, которые парсер не смог прочитать"));
    }
    Ok(chapters)
}
fn annotations(node: &VariationNode, out: &mut Vec<String>) {
    for nag in &node.nags {
        out.push(format!("${nag}"));
    }
    for comment in &node.comments {
        out.push(format!("{{{comment}}}"));
    }
    let mut arrows = vec![];
    let mut circles = vec![];
    for shape in &node.shapes {
        let color = match shape.brush.as_str() {
            "red" => "R",
            "blue" => "B",
            "yellow" => "Y",
            _ => "G",
        };
        if let Some(dest) = &shape.dest {
            arrows.push(format!("{color}{}{dest}", shape.orig));
        } else {
            circles.push(format!("{color}{}", shape.orig));
        }
    }
    if !arrows.is_empty() {
        out.push(format!("{{[%cal {}]}}", arrows.join(",")));
    }
    if !circles.is_empty() {
        out.push(format!("{{[%csl {}]}}", circles.join(",")));
    }
}
fn write_branch(parent: &VariationNode, index: usize, out: &mut Vec<String>) {
    let node = &parent.children[index];
    for text in &node.starting_comments {
        out.push(format!("{{{text}}}"));
    }
    let fields: Vec<_> = parent.fen.split_whitespace().collect();
    let fullmove = fields.get(5).unwrap_or(&"1");
    out.push(format!(
        "{fullmove}{}",
        if fields.get(1) == Some(&"w") {
            "."
        } else {
            "..."
        }
    ));
    out.push(node.san.clone().unwrap_or_default());
    annotations(node, out);
    if index == 0 {
        for i in 1..parent.children.len() {
            let mut variation = vec![];
            write_branch(parent, i, &mut variation);
            out.push(format!("({})", variation.join(" ")));
        }
    }
    if !node.children.is_empty() {
        write_branch(node, 0, out);
    }
}
fn escaped(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\r', '\n'], " ")
}
pub fn export_document(document: &KnowledgeDocument) -> Result<String, AppError> {
    validate_document(document)?;
    let mut chapters = vec![];
    for chapter in &document.chapters {
        let mut tags = chapter.headers.clone();
        tags.insert("ChapterName".into(), chapter.title.clone());
        tags.insert("Event".into(), document.title.clone());
        tags.entry("Result".into()).or_insert_with(|| "*".into());
        if chapter.initial_fen != INITIAL_FEN {
            tags.insert("SetUp".into(), "1".into());
            tags.insert("FEN".into(), chapter.initial_fen.clone());
        } else {
            tags.remove("SetUp");
            tags.remove("FEN");
        }
        let mut text = tags
            .into_iter()
            .map(|(k, v)| format!("[{k} \"{}\"]", escaped(&v)))
            .collect::<Vec<_>>()
            .join("\n");
        let mut moves = vec![];
        annotations(&chapter.root, &mut moves);
        if !chapter.root.children.is_empty() {
            write_branch(&chapter.root, 0, &mut moves);
        }
        moves.push(
            chapter
                .headers
                .get("Result")
                .cloned()
                .unwrap_or_else(|| "*".into()),
        );
        text.push_str("\n\n");
        text.push_str(&moves.join(" "));
        chapters.push(text);
    }
    Ok(chapters.join("\n\n"))
}

fn validate_shape(shape: &Shape) -> Result<(), AppError> {
    if !["green", "red", "blue", "yellow"].contains(&shape.brush.as_str())
        || !square(&shape.orig)
        || shape.dest.as_deref().is_some_and(|d| !square(d))
    {
        return Err(error("Некорректная графическая аннотация"));
    }
    Ok(())
}
fn validate_links(links: &[KnowledgeLink]) -> Result<(), AppError> {
    for link in links {
        if link.provenance.trim().is_empty() {
            return Err(error("Для ссылки требуется происхождение"));
        }
        match &link.target {
            LinkTarget::Position { position_key } if !position_key.starts_with("standard|") => {
                return Err(error("Некорректный позиционный ключ"));
            }
            LinkTarget::Occurrence { occurrence }
                if occurrence.game_id.is_empty() || occurrence.revision < 1 =>
            {
                return Err(error("Некорректная ссылка на партию"));
            }
            LinkTarget::DocumentNode {
                document_id,
                revision,
                chapter_id,
                node_id,
            } if document_id.is_empty()
                || *revision < 1
                || chapter_id.is_empty()
                || node_id.is_empty() =>
            {
                return Err(error("Некорректная ссылка на документ"));
            }
            _ => {}
        }
        if !link.path_uci.is_empty() {
            let mut pos = position(link.initial_fen.as_deref().unwrap_or(INITIAL_FEN))?;
            for token in &link.path_uci {
                let mv = token
                    .parse::<UciMove>()
                    .map_err(error)?
                    .to_move(&pos)
                    .map_err(error)?;
                pos.play_unchecked(mv);
            }
            if let LinkTarget::Position { position_key: key } = &link.target
                && &position_key(&pos) != key
            {
                return Err(error("Путь ссылки не соответствует позиции"));
            }
        }
    }
    Ok(())
}
fn validate_document(document: &KnowledgeDocument) -> Result<(), AppError> {
    if !["repertoire", "study"].contains(&document.kind.as_str())
        || !["white", "black", "both"].contains(&document.side.as_str())
        || document.title.trim().is_empty()
        || document.title.len() > 500
        || document.chapters.is_empty()
        || document.chapters.len() > MAX_CHAPTERS
    {
        return Err(error("Некорректные свойства документа"));
    }
    fn walk(
        node: &VariationNode,
        parent: Option<&VariationNode>,
        depth: usize,
        ids: &mut HashSet<String>,
        count: &mut usize,
    ) -> Result<(), AppError> {
        *count += 1;
        if depth > MAX_DEPTH || *count > MAX_NODES || !ids.insert(node.id.clone()) {
            return Err(error("Повторяющийся ID или слишком большое дерево"));
        }
        let pos = position(&node.fen)?;
        if node.position_key != position_key(&pos) {
            return Err(error("Позиционный ключ не соответствует FEN"));
        }
        if let Some(parent) = parent {
            let mut before = position(&parent.fen)?;
            let mv = node
                .uci
                .as_deref()
                .ok_or_else(|| error("У хода отсутствует UCI"))?
                .parse::<UciMove>()
                .map_err(error)?
                .to_move(&before)
                .map_err(error)?;
            let san = SanPlus::from_move(before.clone(), mv).to_string();
            before.play_unchecked(mv);
            if node.ply != parent.ply + 1
                || node.san.as_deref() != Some(&san)
                || Fen::from_position(&before, EnPassantMode::Always).to_string() != node.fen
            {
                return Err(error("Несогласованный узел варианта"));
            }
        } else if node.uci.is_some() || node.san.is_some() || node.ply != 0 {
            return Err(error("Некорректный корень главы"));
        }
        for text in node.comments.iter().chain(node.starting_comments.iter()) {
            if text.contains(['{', '}']) {
                return Err(error("Комментарий содержит скобки PGN"));
            }
        }
        for shape in &node.shapes {
            validate_shape(shape)?;
        }
        let mut moves = HashSet::new();
        for child in &node.children {
            if !moves.insert(child.uci.as_deref()) {
                return Err(error("Дублированный ход в одной позиции дерева"));
            }
            walk(child, Some(node), depth + 1, ids, count)?;
        }
        Ok(())
    }
    let mut ids = HashSet::new();
    let mut count = 0;
    for chapter in &document.chapters {
        if !ids.insert(chapter.id.clone()) {
            return Err(error("Повторяющийся ID главы"));
        }
        if chapter.initial_fen != chapter.root.fen {
            return Err(error("Корень главы не соответствует FEN"));
        }
        validate_links(&chapter.links)?;
        walk(&chapter.root, None, 0, &mut ids, &mut count)?;
    }
    Ok(())
}

// Flat on disk to avoid serde_json's nested-input recursion limit for long legal lines.
// This is a storage projection of the same VariationTree, never a second editable model.
#[derive(Serialize, Deserialize)]
struct DiskNode {
    value: VariationNode,
    children: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct DiskChapter {
    header: Chapter,
    nodes: Vec<DiskNode>,
}
#[derive(Serialize, Deserialize)]
struct DiskDocument {
    header: KnowledgeDocument,
    chapters: Vec<DiskChapter>,
}
fn encode(document: &KnowledgeDocument) -> Result<String, AppError> {
    fn flatten(node: &VariationNode, nodes: &mut Vec<DiskNode>) {
        let value = VariationNode {
            id: node.id.clone(),
            fen: node.fen.clone(),
            position_key: node.position_key.clone(),
            ply: node.ply,
            uci: node.uci.clone(),
            san: node.san.clone(),
            comments: node.comments.clone(),
            starting_comments: node.starting_comments.clone(),
            nags: node.nags.clone(),
            shapes: node.shapes.clone(),
            children: vec![],
        };
        nodes.push(DiskNode {
            value,
            children: node.children.iter().map(|c| c.id.clone()).collect(),
        });
        for child in &node.children {
            flatten(child, nodes);
        }
    }
    let mut header = document.clone();
    header.chapters.clear();
    let chapters = document
        .chapters
        .iter()
        .map(|c| {
            let mut header = c.clone();
            header.root.children.clear();
            let mut nodes = vec![];
            flatten(&c.root, &mut nodes);
            DiskChapter { header, nodes }
        })
        .collect();
    Ok(serde_json::to_string(&DiskDocument { header, chapters })?)
}
fn decode(json: &str) -> Result<KnowledgeDocument, AppError> {
    fn restore(
        id: &str,
        nodes: &mut BTreeMap<String, DiskNode>,
        depth: usize,
    ) -> Result<VariationNode, AppError> {
        if depth > MAX_DEPTH {
            return Err(error("Слишком глубокий документ"));
        }
        let mut disk = nodes
            .remove(id)
            .ok_or_else(|| error("Повреждённое дерево документа"))?;
        for child in disk.children {
            disk.value.children.push(restore(&child, nodes, depth + 1)?);
        }
        Ok(disk.value)
    }
    let disk: DiskDocument = serde_json::from_str(json)?;
    let mut document = disk.header;
    for c in disk.chapters {
        let mut header = c.header;
        let mut nodes = c
            .nodes
            .into_iter()
            .map(|n| (n.value.id.clone(), n))
            .collect::<BTreeMap<_, _>>();
        header.root = restore(&header.root.id, &mut nodes, 0)?;
        if !nodes.is_empty() {
            return Err(error("Несвязанные узлы документа"));
        }
        document.chapters.push(header);
    }
    Ok(document)
}
fn reindex(conn: &Connection, document: &KnowledgeDocument) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM knowledge_node_index WHERE document_id=?1",
        [&document.id],
    )?;
    fn visit(
        conn: &Connection,
        document: &KnowledgeDocument,
        chapter: &Chapter,
        node: &VariationNode,
        parent: Option<&str>,
    ) -> Result<(), AppError> {
        conn.execute("INSERT INTO knowledge_node_index(document_id,chapter_id,node_id,document_revision,position_key,parent_position_key,uci) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![document.id,chapter.id,node.id,document.revision,node.position_key,parent,node.uci])?;
        for child in &node.children {
            visit(conn, document, chapter, child, Some(&node.position_key))?;
        }
        Ok(())
    }
    for c in &document.chapters {
        visit(conn, document, c, &c.root, None)?;
    }
    Ok(())
}
fn persist(
    conn: &Connection,
    document: &KnowledgeDocument,
    expected: Option<i64>,
) -> Result<(), AppError> {
    validate_document(document)?;
    let json = encode(document)?;
    let tx = conn.unchecked_transaction()?;
    if let Some(expected) = expected {
        let changed=tx.execute("UPDATE knowledge_documents SET kind=?2,title=?3,side=?4,revision=?5,updated_at_ms=?6,data_json=?7 WHERE id=?1 AND revision=?8",params![document.id,document.kind,document.title,document.side,document.revision,document.updated_at_ms,json,expected])?;
        if changed != 1 {
            return Err(AppError::new(
                "conflict",
                "Документ изменён в другом редакторе. Откройте актуальную версию.",
            ));
        }
    } else {
        tx.execute("INSERT INTO knowledge_documents(id,kind,title,side,revision,created_at_ms,updated_at_ms,data_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![document.id,document.kind,document.title,document.side,document.revision,document.created_at_ms,document.updated_at_ms,json])?;
    }
    tx.execute("INSERT INTO knowledge_document_versions(document_id,revision,data_json,created_at_ms) VALUES(?1,?2,?3,?4)",params![document.id,document.revision,json,document.updated_at_ms])?;
    reindex(&tx, document)?;
    tx.commit()?;
    Ok(())
}
pub fn list(conn: &Connection) -> Result<Vec<DocumentSummary>, AppError> {
    let mut statement =
        conn.prepare("SELECT data_json FROM knowledge_documents ORDER BY updated_at_ms DESC,id")?;
    let mut result = vec![];
    for json in statement.query_map([], |row| row.get::<_, String>(0))? {
        let document = decode(&json?)?;
        result.push(fragments::summary(&document));
    }
    Ok(result)
}

pub fn load(
    conn: &Connection,
    id: &str,
    revision: Option<i64>,
) -> Result<KnowledgeDocument, AppError> {
    let json: Option<String> = if let Some(revision) = revision {
        conn.query_row("SELECT data_json FROM knowledge_document_versions WHERE document_id=?1 AND revision=?2",params![id,revision],|r|r.get(0)).optional()?
    } else {
        conn.query_row(
            "SELECT data_json FROM knowledge_documents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
    };
    decode(&json.ok_or_else(|| AppError::new("not_found", "Документ или его ревизия не найдены"))?)
}
pub fn create(conn: &Connection, request: CreateRequest) -> Result<KnowledgeDocument, AppError> {
    let timestamp = now_ms();
    let document = KnowledgeDocument {
        id: new_id("knowledge")?,
        kind: request.kind,
        title: request.title.trim().into(),
        side: request.side,
        revision: 1,
        created_at_ms: timestamp,
        updated_at_ms: timestamp,
        metadata: BTreeMap::new(),
        chapters: vec![chapter(
            "Глава 1".into(),
            request.initial_fen.as_deref(),
            request.links,
        )?],
    };
    persist(conn, &document, None)?;
    Ok(document)
}
pub fn import(conn: &Connection, request: ImportRequest) -> Result<KnowledgeDocument, AppError> {
    let chapters = parse_pgn(&request.pgn)?;
    let (mut document, expected) = if let Some(id) = request.document_id {
        let document = load(conn, &id, None)?;
        let expected = request
            .expected_revision
            .ok_or_else(|| error("Не указана ревизия документа"))?;
        if document.revision != expected {
            return Err(AppError::new("conflict", "Документ уже изменён"));
        }
        (document, Some(expected))
    } else {
        let timestamp = now_ms();
        (
            KnowledgeDocument {
                id: new_id("knowledge")?,
                kind: "repertoire".into(),
                title: request.title.trim().into(),
                side: request.side,
                revision: 0,
                created_at_ms: timestamp,
                updated_at_ms: timestamp,
                metadata: BTreeMap::new(),
                chapters: vec![],
            },
            None,
        )
    };
    document.chapters.extend(chapters);
    document.revision += 1;
    document.updated_at_ms = now_ms();
    persist(conn, &document, expected)?;
    Ok(document)
}
pub fn apply(
    conn: &Connection,
    id: &str,
    expected: i64,
    operation: EditOperation,
) -> Result<KnowledgeDocument, AppError> {
    let mut document = load(conn, id, None)?;
    if document.revision != expected {
        return Err(AppError::new(
            "conflict",
            "Документ уже изменён. Обновите редактор.",
        ));
    }
    match operation {
        EditOperation::Rename { title, side } => {
            document.title = title.trim().into();
            document.side = side;
        }
        EditOperation::AddChapter {
            title,
            initial_fen,
            links,
        } => document
            .chapters
            .push(chapter(title, initial_fen.as_deref(), links)?),
        EditOperation::RemoveChapter { chapter_id } => {
            if document.chapters.len() == 1 {
                return Err(error("В документе должна остаться глава"));
            }
            let index = document
                .chapters
                .iter()
                .position(|c| c.id == chapter_id)
                .ok_or_else(|| error("Глава не найдена"))?;
            document.chapters.remove(index);
        }
        operation => {
            let chapter_id = match &operation {
                EditOperation::AddMove { chapter_id, .. }
                | EditOperation::RemoveVariation { chapter_id, .. }
                | EditOperation::PromoteVariation { chapter_id, .. }
                | EditOperation::Annotate { chapter_id, .. }
                | EditOperation::RenameChapter { chapter_id, .. } => chapter_id,
                _ => unreachable!(),
            };
            let chapter = document
                .chapters
                .iter_mut()
                .find(|c| &c.id == chapter_id)
                .ok_or_else(|| error("Глава не найдена"))?;
            let promote = matches!(&operation, EditOperation::PromoteVariation { .. });
            match operation {
                EditOperation::RenameChapter { title, .. } => chapter.title = title,
                EditOperation::AddMove { parent_id, uci, .. } => {
                    let path = node_path(&chapter.root, &parent_id)?;
                    let parent = at_mut(&mut chapter.root, &path);
                    let mut pos = position(&parent.fen)?;
                    let mv = uci
                        .parse::<UciMove>()
                        .map_err(error)?
                        .to_move(&pos)
                        .map_err(error)?;
                    let uci = UciMove::from_move(mv, CastlingMode::Standard).to_string();
                    if !parent
                        .children
                        .iter()
                        .any(|c| c.uci.as_deref() == Some(&uci))
                    {
                        let san = SanPlus::from_move(pos.clone(), mv).to_string();
                        pos.play_unchecked(mv);
                        parent
                            .children
                            .push(node(&pos, parent.ply + 1, Some(uci), Some(san))?);
                    }
                }
                EditOperation::RemoveVariation { node_id, .. }
                | EditOperation::PromoteVariation { node_id, .. } => {
                    let mut path = node_path(&chapter.root, &node_id)?;
                    let index = path
                        .pop()
                        .ok_or_else(|| error("Корень главы нельзя удалить или повысить"))?;
                    let parent = at_mut(&mut chapter.root, &path);
                    let child = parent.children.remove(index);
                    if promote {
                        parent.children.insert(0, child);
                    }
                }
                EditOperation::Annotate {
                    node_id,
                    comments,
                    nags,
                    shapes,
                    ..
                } => {
                    let path = node_path(&chapter.root, &node_id)?;
                    let node = at_mut(&mut chapter.root, &path);
                    node.comments = comments;
                    node.nags = nags;
                    node.shapes = shapes;
                }
                _ => unreachable!(),
            }
        }
    }
    document.revision += 1;
    document.updated_at_ms = now_ms();
    persist(conn, &document, Some(expected))?;
    Ok(document)
}
pub fn navigate(
    conn: &Connection,
    id: &str,
    chapter_id: &str,
    node_id: &str,
    revision: Option<i64>,
) -> Result<Navigation, AppError> {
    let document = load(conn, id, revision)?;
    let chapter = document
        .chapters
        .iter()
        .find(|c| c.id == chapter_id)
        .ok_or_else(|| error("Глава не найдена"))?;
    let path = node_path(&chapter.root, node_id)?;
    let node = at(&chapter.root, &path);
    let pos = position(&node.fen)?;
    let mut path_node_ids = vec![chapter.root.id.clone()];
    let mut path_uci = vec![];
    let mut current = &chapter.root;
    for i in &path {
        current = &current.children[*i];
        path_node_ids.push(current.id.clone());
        path_uci.push(current.uci.clone().unwrap());
    }
    let legal_moves = pos
        .legal_moves()
        .iter()
        .map(|mv| LegalMove {
            uci: UciMove::from_move(*mv, CastlingMode::Standard).to_string(),
            san: SanPlus::from_move(pos.clone(), *mv).to_string(),
        })
        .collect();
    Ok(Navigation {
        document_id: document.id,
        revision: document.revision,
        chapter_id: chapter.id.clone(),
        node_id: node.id.clone(),
        path_node_ids,
        path_uci,
        initial_fen: chapter.initial_fen.clone(),
        fen: node.fen.clone(),
        position_key: node.position_key.clone(),
        turn: if pos.turn() == Color::White {
            "white"
        } else {
            "black"
        }
        .into(),
        legal_moves,
    })
}
pub fn overlay(conn: &Connection, document_ids: &[String]) -> Result<Vec<PreparedMove>, AppError> {
    fn walk(
        document: &KnowledgeDocument,
        chapter: &Chapter,
        parent: &VariationNode,
        path: &mut Vec<String>,
        out: &mut Vec<PreparedMove>,
    ) {
        for node in &parent.children {
            path.push(node.uci.clone().unwrap());
            out.push(PreparedMove {
                document_id: document.id.clone(),
                document_revision: document.revision,
                chapter_id: chapter.id.clone(),
                node_id: node.id.clone(),
                parent_node_id: parent.id.clone(),
                from_key: parent.position_key.clone(),
                to_key: node.position_key.clone(),
                fen: node.fen.clone(),
                uci: node.uci.clone().unwrap(),
                san: node.san.clone().unwrap(),
                side: document.side.clone(),
                initial_fen: chapter.initial_fen.clone(),
                path_uci: path.clone(),
                links: chapter.links.clone(),
            });
            walk(document, chapter, node, path, out);
            path.pop();
        }
    }
    let mut out = vec![];
    for id in document_ids {
        let document = load(conn, id, None)?;
        if document.kind != "repertoire"
            || (fragments::is_attachment(&document) && !fragments::is_active(&document))
        {
            continue;
        }
        for chapter in &document.chapters {
            walk(&document, chapter, &chapter.root, &mut vec![], &mut out);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    const TREE: &str = "[Event \"Tree\"]\n[X-Custom \"retained\"]\n\n{Root} 1.e4 $1 {plan [%cal Ge2e4] [%csl Re4] [%eval 0.2]} ({candidate} 1.d4 d5 (1...Nf6)) 1...e5 (1...c5 2.Nf3) 2.Nf3 Nc6 *";
    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/006_knowledge.sql"))
            .unwrap();
        conn
    }
    fn request() -> CreateRequest {
        CreateRequest {
            title: "Preparation".into(),
            kind: "repertoire".into(),
            side: "white".into(),
            initial_fen: None,
            links: vec![],
        }
    }
    fn imported(conn: &Connection) -> KnowledgeDocument {
        import(
            conn,
            ImportRequest {
                title: "Preparation".into(),
                side: "white".into(),
                pgn: TREE.into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap()
    }
    fn semantic(node: &VariationNode) -> Value {
        serde_json::json!({"fen":node.fen,"key":node.position_key,"uci":node.uci,"san":node.san,"comments":node.comments,"starting_comments":node.starting_comments,"nags":node.nags,"shapes":node.shapes,"children":node.children.iter().map(semantic).collect::<Vec<_>>()})
    }
    #[test]
    fn nested_rav_annotations_unknown_tags_and_graphics_roundtrip() {
        let conn = db();
        let document = imported(&conn);
        let chapter = &document.chapters[0];
        assert_eq!(chapter.root.children.len(), 2);
        assert_eq!(chapter.root.children[1].children.len(), 2);
        assert_eq!(chapter.root.children[0].children.len(), 2);
        let e4 = &chapter.root.children[0];
        assert_eq!(e4.nags, vec![1]);
        assert_eq!(e4.shapes.len(), 2);
        assert!(e4.comments[0].contains("[%eval 0.2]"));
        let export = export_document(&document).unwrap();
        let parsed = parse_pgn(&export).unwrap();
        assert_eq!(semantic(&parsed[0].root), semantic(&chapter.root));
        assert_eq!(parsed[0].headers.get("X-Custom").unwrap(), "retained");
        assert_eq!(chapter.source_pgn.as_deref(), Some(TREE));
    }
    #[test]
    fn multi_chapter_custom_fen_and_black_turn_roundtrip() {
        let conn = db();
        let pgn = format!(
            "{TREE}\n\n[Event \"Ending\"]\n[SetUp \"1\"]\n[FEN \"4k3/8/8/8/8/8/8/4K2R b K - 18 17\"]\n\n17... Kd7 18.Rh7+ *"
        );
        let document = import(
            &conn,
            ImportRequest {
                title: "Two".into(),
                side: "both".into(),
                pgn,
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        assert_eq!(document.chapters.len(), 2);
        let roundtrip = parse_pgn(&export_document(&document).unwrap()).unwrap();
        assert_eq!(
            semantic(&roundtrip[1].root),
            semantic(&document.chapters[1].root)
        );
        assert_eq!(
            document.chapters[1].root.children[0].san.as_deref(),
            Some("Kd7")
        );
        assert!(export_document(&document).unwrap().contains("17..."));
    }
    #[test]
    fn mainline_promotion_branch_removal_and_historical_links_keep_ids() {
        let conn = db();
        let initial = imported(&conn);
        let c = &initial.chapters[0];
        let original_e4 = c.root.children[0].id.clone();
        let d4 = c.root.children[1].id.clone();
        let promoted = apply(
            &conn,
            &initial.id,
            1,
            EditOperation::PromoteVariation {
                chapter_id: c.id.clone(),
                node_id: d4.clone(),
            },
        )
        .unwrap();
        assert_eq!(promoted.chapters[0].root.children[0].id, d4);
        assert_eq!(promoted.chapters[0].root.children[1].id, original_e4);
        let removed = apply(
            &conn,
            &initial.id,
            2,
            EditOperation::RemoveVariation {
                chapter_id: c.id.clone(),
                node_id: original_e4.clone(),
            },
        )
        .unwrap();
        assert_eq!(removed.chapters[0].root.children.len(), 1);
        assert!(navigate(&conn, &initial.id, &c.id, &original_e4, None).is_err());
        assert_eq!(
            navigate(&conn, &initial.id, &c.id, &original_e4, Some(1))
                .unwrap()
                .node_id,
            original_e4
        );
        assert_eq!(load(&conn, &initial.id, Some(1)).unwrap(), initial);
    }
    #[test]
    fn legal_board_edit_is_idempotent_and_illegal_edit_is_atomic() {
        let conn = db();
        let initial = create(&conn, request()).unwrap();
        let c = &initial.chapters[0];
        let op = EditOperation::AddMove {
            chapter_id: c.id.clone(),
            parent_id: c.root.id.clone(),
            uci: "e2e4".into(),
        };
        let edited = apply(&conn, &initial.id, 1, op.clone()).unwrap();
        let e4 = edited.chapters[0].root.children[0].id.clone();
        let repeated = apply(&conn, &initial.id, 2, op).unwrap();
        assert_eq!(repeated.chapters[0].root.children.len(), 1);
        assert_eq!(repeated.chapters[0].root.children[0].id, e4);
        assert!(
            apply(
                &conn,
                &initial.id,
                3,
                EditOperation::AddMove {
                    chapter_id: c.id.clone(),
                    parent_id: c.root.id.clone(),
                    uci: "e2e5".into()
                }
            )
            .is_err()
        );
        assert_eq!(load(&conn, &initial.id, None).unwrap().revision, 3);
        let nav = navigate(&conn, &initial.id, &c.id, &e4, None).unwrap();
        assert_eq!(nav.path_uci, vec!["e2e4"]);
        assert_eq!(nav.turn, "black");
        assert_eq!(nav.legal_moves.len(), 20);
    }
    #[test]
    fn optimistic_conflict_does_not_overwrite_or_add_version() {
        let conn = db();
        let doc = create(&conn, request()).unwrap();
        apply(
            &conn,
            &doc.id,
            1,
            EditOperation::Rename {
                title: "New".into(),
                side: "black".into(),
            },
        )
        .unwrap();
        let err = apply(
            &conn,
            &doc.id,
            1,
            EditOperation::Rename {
                title: "Stale".into(),
                side: "white".into(),
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "conflict");
        assert_eq!(load(&conn, &doc.id, None).unwrap().title, "New");
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM knowledge_document_versions",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }
    #[test]
    fn restart_retains_documents_chapters_nodes_and_annotations() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("knowledge.sqlite3");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/006_knowledge.sql"))
            .unwrap();
        let doc = imported(&conn);
        drop(conn);
        let reopened = Connection::open(&path).unwrap();
        let loaded = load(&reopened, &doc.id, None).unwrap();
        assert_eq!(doc, loaded);
        assert_eq!(
            overlay(&reopened, std::slice::from_ref(&doc.id))
                .unwrap()
                .len(),
            9
        );
        assert_eq!(list(&reopened).unwrap()[0].chapters, 1);
    }
    #[test]
    fn transpositions_share_position_not_tree_identity_and_cycles_are_finite() {
        let chapters = parse_pgn(
            "1.Nf3 Nf6 2.Nc3 Nc6 (2...Ng8 3.Ng1 Nf6) (2...d5) *\n\n1.Nc3 Nc6 2.Nf3 Nf6 *",
        )
        .unwrap();
        let a = &chapters[0].root.children[0].children[0].children[0].children[0];
        let b = &chapters[1].root.children[0].children[0].children[0].children[0];
        assert_eq!(a.position_key, b.position_key);
        assert_ne!(a.id, b.id);
        let repeated = parse_pgn("1.Nf3 Nf6 2.Ng1 Ng8 *").unwrap();
        let mut node = &repeated[0].root;
        for _ in 0..4 {
            node = &node.children[0];
        }
        assert_eq!(node.position_key, repeated[0].root.position_key);
        assert_ne!(node.id, repeated[0].root.id);
    }
    #[test]
    fn long_line_persists_without_nested_json_recursion_limit() {
        let conn = db();
        let pgn = format!("{} *", "Nf3 Nf6 Ng1 Ng8 ".repeat(50));
        let doc = import(
            &conn,
            ImportRequest {
                title: "Long".into(),
                side: "both".into(),
                pgn,
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        let reloaded = load(&conn, &doc.id, None).unwrap();
        let mut leaf = &reloaded.chapters[0].root;
        for _ in 0..200 {
            leaf = &leaf.children[0];
        }
        assert_eq!(leaf.ply, 200);
        assert_eq!(leaf.position_key, reloaded.chapters[0].root.position_key);
    }
    #[test]
    fn invalid_imports_do_not_leave_partial_documents() {
        let conn = db();
        for bad in [
            "1.e5 *",
            "1.e4 (1.d4 *",
            "1.e4 blabla *",
            "1.e4 {unclosed",
            "1.e4 ) *",
            "[Variant \"Atomic\"]\n1.e4 *",
        ] {
            assert!(
                import(
                    &conn,
                    ImportRequest {
                        title: "Invalid".into(),
                        side: "white".into(),
                        pgn: bad.into(),
                        document_id: None,
                        expected_revision: None
                    }
                )
                .is_err(),
                "{bad}"
            );
        }
        assert!(list(&conn).unwrap().is_empty());
    }
    #[test]
    fn repertoire_and_future_study_use_same_api_and_do_not_touch_factual_games() {
        let conn = db();
        conn.execute("INSERT INTO games(identity,metadata,tags,pgn,source,source_kind) VALUES('fixture','{}','{}','1.e4 e5 *','fixture','\"local\"')",[]).unwrap();
        let primary = conn
            .query_row("SELECT pgn FROM games", [], |r| r.get::<_, String>(0))
            .unwrap();
        let mut request = request();
        request.kind = "study".into();
        let study = create(&conn, request).unwrap();
        let repertoire = imported(&conn);
        assert_eq!(
            study.chapters[0].root.position_key,
            repertoire.chapters[0].root.position_key
        );
        let edited_study = apply(
            &conn,
            &study.id,
            1,
            EditOperation::AddMove {
                chapter_id: study.chapters[0].id.clone(),
                parent_id: study.chapters[0].root.id.clone(),
                uci: "d2d4".into(),
            },
        )
        .unwrap();
        assert_eq!(
            parse_pgn(&export_document(&edited_study).unwrap()).unwrap()[0]
                .root
                .children[0]
                .uci
                .as_deref(),
            Some("d2d4")
        );
        assert!(overlay(&conn, &[study.id]).unwrap().is_empty());
        let original_key = repertoire.chapters[0].root.position_key.clone();
        let removed = apply(
            &conn,
            &repertoire.id,
            1,
            EditOperation::RemoveVariation {
                chapter_id: repertoire.chapters[0].id.clone(),
                node_id: repertoire.chapters[0].root.children[0].id.clone(),
            },
        )
        .unwrap();
        assert_eq!(removed.chapters[0].root.position_key, original_key);
        assert_eq!(
            conn.query_row("SELECT pgn FROM games", [], |r| r.get::<_, String>(0))
                .unwrap(),
            primary
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM games", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn chapters_append_and_stable_position_occurrence_links_survive() {
        let conn = db();
        let mut req = request();
        req.links = vec![KnowledgeLink {
            target: LinkTarget::Occurrence {
                occurrence: OccurrenceRef {
                    game_id: "lichess:test".into(),
                    revision: 3,
                    ply: 4,
                },
            },
            provenance: "observed_graph".into(),
            path_uci: vec!["g1f3".into(), "g8f6".into(), "f3g1".into(), "f6g8".into()],
            initial_fen: Some(INITIAL_FEN.into()),
        }];
        let doc = create(&conn, req).unwrap();
        let chapter_id = doc.chapters[0].id.clone();
        let imported = import(
            &conn,
            ImportRequest {
                title: "ignored".into(),
                side: "black".into(),
                pgn: TREE.into(),
                document_id: Some(doc.id.clone()),
                expected_revision: Some(1),
            },
        )
        .unwrap();
        assert_eq!(imported.chapters.len(), 2);
        assert_eq!(imported.chapters[0].id, chapter_id);
        assert_eq!(imported.side, "white");
        assert_eq!(imported.chapters[0].links, doc.chapters[0].links);
    }
    #[test]
    fn semicolon_comments_castling_and_unknown_directives_preserve_semantics() {
        let conn = db();
        let pgn = "[X-Source \"with;semicolon\"]\n\n1.e4$1 ;first note\n1...e5 2.Nf3 Nc6 3.Bc4 Nf6 4.0-0 {[%unknown retain-me]} *";
        let doc = import(
            &conn,
            ImportRequest {
                title: "Comments".into(),
                side: "white".into(),
                pgn: pgn.into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        assert_eq!(
            doc.chapters[0].root.children[0].comments,
            vec!["first note"]
        );
        let exported = export_document(&doc).unwrap();
        assert!(exported.contains("[%unknown retain-me]"));
        assert!(exported.contains("O-O"));
        assert_eq!(
            semantic(&parse_pgn(&exported).unwrap()[0].root),
            semantic(&doc.chapters[0].root)
        );
    }
    #[test]
    fn duplicate_move_branches_merge_annotations_without_replacing_mainline() {
        let chapters = parse_pgn("1.e4 {A} (1.e4 {B} c5) e5 *").unwrap();
        let root = &chapters[0].root;
        assert_eq!(root.children.len(), 1);
        let e4 = &root.children[0];
        assert_eq!(e4.comments, vec!["A", "B"]);
        assert_eq!(e4.children[0].uci.as_deref(), Some("e7e5"));
        assert_eq!(e4.children[1].uci.as_deref(), Some("c7c5"));
        let separate =
            parse_pgn("[Event \"A\"]\n1.e4 {A} e5 *\n\n[Event \"B\"]\n1.e4 {B} c5 *").unwrap();
        assert_eq!(separate.len(), 2);
        assert_eq!(
            separate[0].root.children[0].position_key,
            separate[1].root.children[0].position_key
        );
        assert_ne!(separate[0].id, separate[1].id);
        assert_eq!(separate[0].root.children[0].comments, vec!["A"]);
        assert_eq!(separate[1].root.children[0].comments, vec!["B"]);
    }
    #[test]
    fn strict_header_and_result_errors_are_explicit() {
        for pgn in [
            "[Event]\n1.e4 *",
            "[Event \"A\"]\n[Event \"B\"]\n1.e4 *",
            "[Result \"1-0\"]\n1.e4 0-1",
            "1.e4 () *",
            "1.e4 ;note { with braces }\n*",
        ] {
            assert!(parse_pgn(pgn).is_err(), "unexpectedly accepted {pgn}");
        }
    }
}
