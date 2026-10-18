# Graph / Knowledge implementation notes

Implemented Graph and Repertoire contracts, acceptance checks and measured limits. The native UI reads actual persisted account games; synthetic games are confined to isolated QA profiles.

## Shared editable chess content

`src-tauri/src/knowledge` is a headless domain/application API shared by repertoire and future study documents. `KnowledgeDocument.kind` currently supports `repertoire` and `study`; both use exactly the same chapter/tree, navigation, validation, editing, revision and PGN serialization functions. Repertoire is a document role. Observed games and engine observations remain independent primary data.

A document has stable random IDs, title, side, metadata, timestamps and monotonically increasing content revision. Each chapter owns an initial FEN, PGN headers, stable root/node IDs, a legal VariationTree, comments, NAG, graphics, and typed provenance links. Child zero is the main continuation. Different chapters are never merged by position; transposed positions share the canonical key without losing their independent tree/history context.

The Rust `shakmaty` replay is authoritative for legal changes and SAN/UCI. `chess_domain::position_key` is reused for factual and prepared positions. `knowledge_navigate` returns full FEN, actual path and backend-validated legal UCI destinations. The UI does not parse PGN or own a competing variation model. Existing linear BoardSession remains a presentation adapter.

Implemented headless operations and Tauri commands: create, load/list, append imported PGN chapters, add a move/alternative, delete a subtree, promote a main continuation, annotate, rename document/chapter, add/remove chapters, navigate, export and load an explicit historical revision. Editing uses optimistic revision checks; stale writes return `conflict`. Removing/promoting a branch preserves all surviving IDs. Historical snapshots can still resolve removed node IDs under their original revision.

## Storage and factual isolation

Migration **006_knowledge.sql** adds `knowledge_documents`, immutable `knowledge_document_versions` snapshots and derived `knowledge_node_index`. SQLite updates the live document, snapshot and node index in one transaction. The tree is stored as a flat serialization projection to avoid serde JSON's nested-input recursion limit on long legal lines; it remains the same editable domain model returned by the API. No repertoire-specific PGN storage format or observed-game writer is introduced.

The repertoire overlay is a read-only projection of selected `kind=repertoire` documents: document/revision/chapter/node provenance, canonical source/destination positions, prepared UCI/SAN, full FEN, root and path context. It has no factual counters. Creating/editing/deleting prepared branches does not write games, memberships, payloads or engine evidence.

## PGN support and explicit limits

Import supports standard chess, custom legal FEN (including black to move), multiple chapters, nested RAV, comments, NAG, mainline/side-line structure, standard coloured arrows/circles (`%cal` / `%csl`) and unknown header/comment directives. The original imported PGN batch is retained once in the first imported chapter; normalized semantic PGN is exported. Duplicate move branches inside one chapter merge annotations and descendants while preserving the higher-priority mainline. Different chapter/provenance contexts remain separate.

The tolerant third-party PGN reader is wrapped with explicit lexical validation, balanced-delimiter checks, header checks, legal SAN replay and a count comparison against visited moves. Unsupported variants/tokens, malformed headers/RAV, conflicting results and illegal moves fail the import before any document is stored. Semicolon comments are normalized into supported comment records and re-exported. Unrecognized comment directives are preserved as text. Semicolon comments containing literal brace delimiters are explicitly rejected rather than silently changed.

Operational input guards: 10 MiB per PGN import, 256 chapters, 50,000 nodes per document, 512 plies/depth. Unsupported chess variants and null-move notation are rejected. Stable IDs survive storage/edit/restart; importing an exported PGN as a new document intentionally creates new IDs while preserving semantic chess content.

## Core verification status

Final backend verification completed successfully:

- `cargo fmt --manifest-path src-tauri/Cargo.toml` completed.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --no-default-features --all-targets -- -D warnings` passed without warnings.
- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features` passed: 170 unit tests, 12 integration tests (3 legacy-import, 9 parser/parity), zero failures. Three opt-in real-Stockfish tests were ignored by this command and subsequently passed in an explicit actual-engine run. Doctests completed with zero cases. This run also built the examples successfully.
- The unit totals include all 14 Knowledge contract tests and all 12 graph contract tests. Knowledge coverage includes nested RAV/annotations/custom FEN, legal/idempotent edits, stable IDs, historical links after removal, optimistic conflicts, restart, transpositions/repetitions, shared study/repertoire API, factual isolation, a 200-ply persisted line, semicolon comments/zero-digit castling/unknown directives, duplicate-line mainline priority and malformed-header/result errors. Graph tests include synchronized-new-game/membership cache invalidation.

The initial bare test command encountered Windows LNK1104 because the graph benchmark executable was still running; the complete rerun above passed after that process finished. Native GUI acceptance, desktop checks and rendering measurements follow below.

## Future Knowledge scope

The document/tree API and persisted instances are already reusable by a future Knowledge editor without a repertoire conversion. A full Knowledge workspace, rich prose blocks, generalized tag organization, model-game integration, semantic links/recommendations, training and SRS remain future work. Position identity is not treated as a knowledge document or safe UCI cache key.



## Factual graph and shared navigation

Migration **007_game_graph.sql** adds shared `chess_positions`, revision-qualified `game_occurrences`, original line roots, filterable game summaries and opening-label projections, all linked to existing `games`. Ingestion updates this derived index atomically with accepted source payloads. Old observed revisions remain addressable. Initial backfill indexes only missing or changed games; later synchronization indexes each newly accepted game without rebuilding the history. A bounded cache of cohort totals is invalidated by exact selected game/revision/payload/membership fingerprints.

`OccurrenceRef = (game_id, revision, ply)` is shared by documents, graph evidence and resolvers. Repeated positions and cycles retain all visits; unique-game W/D/L counts remain independent. Context resolution returns the original full FEN root and legal path, including the A→B→A payload-deduplication case. `StoredGame.movetext_revision` is provided by the authoritative loader. `BoardSession.loadReplay` adapts Rust-normalized positions for the existing game viewer and graph, avoiding a second frontend PGN replay of factual games.

Queries reuse the Statistics filter semantics and require verified account membership, confirmed completion and a supported variant. Source/material labels never prove personal ownership. The graph returns at most 300 vertices through the backend contract; the current UI requests 200. Local top-K rendering does not restrict available board navigation: all legal observed/prepared continuations of the selected position are returned separately (bounded by 256). Evidence is paginated, and its rows carry game/revision/ply provenance.

The UI provides overview/local expansion, physical node and edge selection, Chessground move navigation, history back/forward, FEN search, zoom/pan/fit/selection focus, path selection and source-game navigation. Existing saved engine evidence is shown separately with White POV, profile, depth and coverage. The repertoire overlay exposes prepared-only, matching, uncovered reply and own-deviation states; deviation is not classified as a playing error. Editing the prepared layer never changes factual counters. A document node can return to the graph with its chapter root and path; an origin link independently retains the observed game path.

Graph layout runs in a worker, retains existing coordinates, handles cycles without recursive traversal and bounds its coordinate cache. Camera/route/overlay are saved per account. Theme changes and reduced motion are supported. The graph and editor load as a separate product chunk (about 471 KB minified; main chunk about 401 KB), so graph libraries do not enlarge the initial application chunk.

## Renderer comparison and selection

Pinned versions verified from package metadata: Cytoscape.js **3.34.3** (MIT), React Flow **12.12.0** (MIT), Sigma **4.0.3** + Graphology **0.26.0** (MIT), ELK.js **0.12.0** (EPL-2.0 OR GPL-3.0-or-later). Product code imports only Cytoscape; other candidates belong to the reproducible development prototype.

Primary references: [Cytoscape API and performance](https://js.cytoscape.org/), [React Flow](https://reactflow.dev/), [Sigma rendering](https://www.sigmajs.org/concepts/rendering/), [ELK worker/layout API](https://github.com/kieler/elkjs). ELK computes layout and is not a renderer.

Reproduce with `npm run dev` followed by `npm run bench:graph-renderers`. The prototype uses the same synthetic directed graph with branches and back edges, a 1000×700 container, three animation frames for initial paint, and 20 pan frames. Measurements use headless Edge on this development machine; they are neither a cross-machine ranking nor a promised FPS.

| Candidate | Ready 100 nodes, ms | Ready 500, ms | Ready 1500, ms | Pan frame median at 1500, ms |
|---|---:|---:|---:|---:|
| Cytoscape | 106.0 | 122.9 | 223.7 | 31.7 |
| React Flow | 200.5 | 328.2 | 731.8 | 16.1 |
| Sigma | 3551.6 cold WebGL | 110.9 warm | 120.5 warm | 7.0 |
| ELK layout only | 150.7 | 231.8 | 688.6 | N/A |

Cytoscape was selected for bounded chess neighborhoods: Canvas node/edge picking, directed cycles, styles for overlays and factual counts, lower initial cost in this prototype, and straightforward preset layout/camera control. React Flow offers convenient React nodes but incurs a higher initial render cost here. Sigma performs best on the dense warm sample, at the cost of cold WebGL initialization and additional label/edge customization. ELK would recompute layout; the implemented incremental worker keeps existing positions steady as branches appear. At 1500 nodes Cytoscape pan latency is material, which supports the explicit 200-node UI limit rather than rendering the complete position index.

## Integration acceptance

- Native build: `npm run tauri build -- --debug --no-bundle` passed. TypeScript and production Vite build passed; the layout worker is bundled separately.
- Frontend: **23 tests across 6 files passed**. Prettier check passed. Full desktop and no-default all-target Clippy passed with `-D warnings`; Rust formatting passed.
- Real Stockfish: all **3 opt-in tests passed**, including UCI/searchmoves, persisted full-game analysis, and lost-mate recheck.
- Native Graph/Repertoire first + restart passed on the final backend build: `.qa/game-graph-native-1VfZ7p/data`. Actual Canvas clicks, board moves, filters, merged transpositions, source-game transitions, four editable chapters, seven manual moves, alternative/mainline changes, branch deletion, custom FEN/promotion, comments/NAG/arrows, PGN export/reimport and unchanged factual results were asserted. Full document/chapter/node IDs and camera zoom/pan were compared after restart.
- Existing Statistics native **four phases passed**: `.qa/statistics-native-ymsjpP`. This includes real engine setup, pause/cancel/recovery, evidence, six original sections, filters, comparisons and persistence.
- API→SQLite→GUI→Stockfish→restart **three phases passed**: `.qa/statistics-flow-N9XiuF`. A first game appeared in an already-open graph automatically, with no Sync click; measured discovery was about **63.56 s** for the regular 60-second polling cycle. Games published while closed were recovered with the same T0, deduplication and transposition semantics.
- Existing native parser/board/account smoke suite passed. It checked exact zst/PGN matches, cancellation, corrupt input, persistence, click/drag/arrows/circles/promotion/themes and auth cancellation without personal credentials. Account browser UI regression and privacy checks passed.

Native GUI checks use the real Tauri/WebView2 app with CDP input and screenshots, not a substitute dashboard. Windows Computer Use capture was attempted separately but returned `FrameArrived timed out` and `window capture timed out`; rendered native screenshots were inspected directly instead. All fixtures and profiles are isolated under `.qa`; the personal database and account were not used.

## Measured scale and operational limits

`game_graph_benchmark` generates independent legal synthetic games. Debug measurements on this machine:

| Games | Visits | Unique positions | Returned nodes/edges | DTO | Warm query median | First new-cohort query |
|---|---:|---:|---:|---:|---:|---:|
| 1000 | 60,566 | 57,827 | 43 / 42 | 53.4 KB | 101–110 ms | 183–221 ms |
| 10,000 | 606,617 | 570,147 | 43 / 42 | 54.3 KB | 991 ms | 5592 ms |

The 10,000-game run added 9000 games to the first 1000: generation/legal replay/persistence took about 171 s, of which transactional save/index work took about 134.6 s. These debug bulk-import figures are not per-game polling latency or release-build predictions. Heavy work runs outside the GUI thread. A first new filtered cohort can still take seconds because the full distinct-position count is computed once; the cache does not eliminate that cold cost.

The final native 1000-game volume scenario (`.qa/game-graph-volume-cJalWj/data`) expanded to **200 vertices / 199 edges**, exercised pan/zoom/board navigation and both themes. The measured interaction workflow took 3.20 s; 155 heartbeat samples gave P99 **85.9 ms**, maximum **87.7 ms**. This final run includes readable zoom-to-selection as well as pan, fit, expansion and board navigation. This is an observed short scenario, not an FPS guarantee. The board/table can navigate continuations outside the rendered top-K; an overview fit can make nodes small, and selection focus restores readable zoom.

Only standard chess is supported. PGN semantic structure is preserved; export normalizes whitespace and formatting rather than reproducing source bytes. Full UI organization of Knowledge, rich prose editing, tags, semantic search/recommendations, model-game integration, training/SRS and catalog O14/O15 integration remain separate future work. Those features can reuse the persisted document/tree and versioned links without converting repertoires into another format.



## Task readiness mapping

| Task §8 | Verified behavior | Evidence |
|---|---|---|
| 1 | Personal completed games or honest empty state | verified-membership SQL/unit tests, native Graph and empty API-flow |
| 2 | Physical Canvas selection and board continuation navigation | `tests/graph-native.mjs`, readable selection zoom in native volume |
| 3 | Unique games, visits, own-POV W/D/L and source-game evidence | graph numerical tests, paged evidence, native source viewer |
| 4 | Transpositions and route-aware back/forward, finite cycles | backend cycle/transposition tests, frontend provenance/history tests, native merged-position paths |
| 5 | Statistics filters change the factual cohort without data loss | filter goldens and native color/control/FEN workflows |
| 6 | Create/import/edit/save/export a shared KnowledgeDocument | 14 core tests, native four-chapter RAV/FEN/annotations workflow |
| 7 | Prepared overlay never alters played counters | backend isolation/overlay tests and native before/after result signatures |
| 8 | Persisted graph/documents and automatic new-game indexing | two native launches, exact stable-ID/camera assertions, live/offline API-flow |
| 9 | Existing Statistics/parser/engine continue working | native Statistics four phases, parser/board/account smoke, all Rust/front-end regressions |
| 10 | Same headless API for repertoire and future study | study/repertoire contract test, historical links and persistent shared tree schema |

Development-only QA fix: Vite ignores `.qa` as well as the Rust build tree. This avoids Windows `EBUSY` watcher crashes on live WebView2 Cookie files. Dev-server HTTP health was verified while an exclusively locked QA file existed; the account UI regression passed again. Production bundle hashes remained unchanged after this watcher-only change.
