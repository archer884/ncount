# ncount — agent notes

A small Rust CLI that computes word/paragraph stats from Markdown, treating
`<!-- -->` HTML comments and `[^footnotes]` as noise. Written to replace a
slower C# version; startup + full-book run is single-digit milliseconds
(~8ms against a full novel), so "high performance" here means "doesn't
interrupt flow state," not raw throughput at scale.

Real test fixtures: the user's actual manuscript lives outside this repo at
`/home/archer/Documents/books/eos-2024/src/*.md` — nine chapter/note files
with real headings, comments, and footnotes. Prefer running against those
over inventing synthetic fixtures; `resource/sample.md` in this repo is a
smaller canned sample for quick checks.

## Pipeline (main.rs)

`cli::Args` has an optional `#[command(subcommand)]` (`Command::Tui`) plus a
`#[command(flatten)] common: CommonArgs` — clap disambiguates a literal
`tui` first token from the top-level `paths: Vec<String>` correctly
(confirmed with a standalone spike before building on it: `ncount tui
foo.md`, `ncount foo.md`, and a file literally named `tui.md` all parse as
expected). No subcommand → `run_once` (below, unchanged behavior); `tui`
subcommand → `tui::run` (see "TUI" section below). `CommonArgs` carries
`paths`/`--filter`/`--verbose` and is shared verbatim by both paths.

`run_once` (classic one-shot mode, `main.rs`):

1. `materialize_files()` resolves paths/globs to files and **sorts** them,
   because non-Windows readdir order is inode order, not name order.
2. `filter::TextFilter` — one compiled regex strips comments/footnotes from
   each file's raw text before anything else sees it.
3. `document::DocumentBuilder` — walks the filtered text line by line,
   building a tree of `Document`s keyed by Markdown heading level (`#`
   depth). Each leading blank-trimmed non-heading line is a "paragraph";
   its word count comes from `count_words()`, which fast-paths pure-ASCII
   lines through a hand-rolled tokenizer and falls back to
   `unicode_words()` for anything else (see below).
4. `fmt::StatFmt` — walks the finished `Document` tree, optionally filters
   to a subtree by heading (case-insensitive prefix match), and renders a
   `prettytable` table to stdout, with a running cumulative word total.

## TUI (`src/tui.rs` + `src/tui/`)

`ncount tui <paths> [-f ...] [-v]` — an interactive, auto-refreshing
alternative to the classic one-shot table, built on `ratatui` + `crossterm`
+ `notify`/`notify-debouncer-mini`. Reuses `document.rs`/`filter.rs`
completely unchanged; `fmt.rs` stays the renderer for the classic path only.

- **One `Document` per resolved file** (`tui/app.rs::App::files`), each
  built through its own fresh `DocumentBuilder` — confirmed earlier in this
  session that a builder is safe to use per-file rather than folded across
  a whole directory. This is what makes watch-mode refresh cheap: a
  file-change event only rebuilds *that* file's `Document`
  (`App::reload`), not the whole tree.
- **`tui/watch.rs`** watches the *parent directories* of resolved files
  (not the exact file paths) via `notify-debouncer-mini`, 300ms debounce —
  required because editors save via write-temp-then-rename (confirmed with
  helix/vim's pattern in a driver test: renaming a new inode over the
  watched path still fires a debounced event when the directory is
  watched). Events are filtered down to the tracked path set and delivered
  through a plain `mpsc::Receiver<DebounceEventResult>` (the crate has a
  builtin `DebounceEventHandler` impl for `mpsc::Sender`, so no manual
  callback/channel plumbing was needed).
- **Filtering** mirrors the CLI's fallback exactly: try `get_heading()`
  against each file's `Document` in order, first match wins; no match shows
  a status-line warning (in-app equivalent of the CLI's stderr warning) and
  falls back to showing everything. `-f`/`--filter` just seeds the initial
  value; `f`/`/` opens a live text-input (vim/helix-style) to replace it at
  runtime, `Enter` applies, `Esc` reverts to whatever was active before.
- **Per-row expand, not a global verbose toggle** — the CLI's `-v` is
  accepted (for `CommonArgs` flag-compatibility) but *ignored* in the TUI,
  per explicit instruction. Every row starts collapsed (Words/Total only).
  `v` **toggles** the selected row (expand if collapsed, collapse if
  expanded); `→`/`←` expand/collapse it directionally (so `→` on an
  already-expanded row is a no-op, unlike `v`). `render.rs` checks whether
  *any* currently-visible row is expanded and swaps between a 3-column
  (`§ Words Total`) and 6-column (`§ Count¶ Avg¶ Long¶ Words Total`) `Table`
  + header for the whole frame — the Count¶/Avg¶/Long¶ header labels only
  exist on screen when something is actually revealed, not as blank columns
  sitting there unused. Chosen over a hand-rolled variable-height list
  because `Table` gets automatic column-width alignment for free (same as
  `prettytable` does today), at the cost of the reveal not literally
  growing/animating one row in place — it widens the whole table instead.
  Expand-state and selection are keyed by `(file_index, heading text)`, not
  row index, so a live refresh that changes row count doesn't scramble
  which rows are expanded.
- **Navigation**: `j`/`k`/`↑`/`↓` move the `ratatui::widgets::TableState`
  selection (which drives scroll-into-view automatically — no hand-rolled
  scroll math). `q`/`Esc`/`Ctrl-C` quit. Ctrl-C needed explicit handling
  since raw mode intercepts the signal (crossterm never delivers a SIGINT
  in raw mode — it arrives as a normal key event with `KeyModifiers::CONTROL`).
- **Terminal safety**: a panic hook (installed in `tui::mod`'s
  `init_terminal`) disables raw mode and leaves the alternate screen before
  the default panic handler runs — without it, a bug in the TUI leaves the
  user's shell stuck. Verified end-to-end (not just by reading the code) by
  driving the compiled binary through a real pty (Python + `pyte`, since no
  tmux/screen was available in this environment) — confirmed clean
  `\x1b[?1049l\x1b[?25h` (leave-alt-screen + show-cursor) in the raw output
  tail after `q`, and exit code 0 after both `q` and Ctrl-C.
- Module layout deliberately mirrors the existing `fmt.rs` + `fmt/heading.rs`
  pattern already in this codebase: `src/tui.rs` (entry point, terminal
  setup/teardown, event loop) + `src/tui/{app,render,watch}.rs`, **not**
  `src/tui/mod.rs` — this project's explicit preference is no `mod.rs`
  files (2018-style module paths only).

## Known issues

Fixed (2026-07-13), since the user confirmed the filter-fallback was the
only one of these actually hit in real usage — the other two were fixed
opportunistically since they were small and already understood:

- `Paragraphs::min` / `OverallStats::min` deleted. They were always 0
  (defaulted via `Default`, and `0.min(p) == 0` for all `p`), and never
  read anywhere in `fmt.rs` — dead and broken, so removed rather than fixed.
- `StatFmt::apply_filter` (fmt.rs) now `eprintln!`s a yellow warning to
  stderr (`no heading matching "..." found; showing everything`) when
  `--filter <heading>` matches nothing, instead of silently falling back
  to the full document. Fallback behavior (still shows everything)
  unchanged — only the silence was the bug, per the user. Coloring uses
  `owo-colors` (`OwoColorize::yellow()`), added as a dependency in place
  of hand-rolled ANSI escapes — zero-dependency crate, chosen by the user
  over `colored` and the already-transitive `anstyle`/`anstream` for a
  nicer `.yellow()` call site.
- Clippy lifetime-elision warning in `document.rs` (`iter(&'_ self) ->
  Box<dyn Iterator<Item = DocumentStats> + '_>`) fixed by spelling out
  `DocumentStats<'_>` explicitly. Not a real bug — clippy got stricter
  since this code was last touched.
- **Added a unit test suite** (`#[cfg(test)] mod tests` in `document.rs`
  and `filter.rs`, 18 tests total) as a characterization safety net before
  touching word-counting — covers heading/level tree building, paragraph
  aggregation, the "text before the first heading is invisible" quirk (see
  below), `get_heading` prefix matching, and comment/footnote/note
  stripping. Run with `cargo test`.
- **Added an ASCII fast path for word counting** (`document.rs`:
  `count_words` / `ascii_word_count`), profiled and measured, not just
  theorized:
  - Profiling the real binary (temporary `Instant` timers, since reverted)
    against the user's actual book showed `DocumentBuilder::apply`
    (tree-building + word counting) at ~83% of total runtime, dwarfing
    disk I/O (~1%) and comment/footnote regex filtering (~13%).
    `unicode_words()`'s full UAX#29 segmentation was the dominant cost
    within that.
  - Checked for a drop-in faster crate first (`finl_unicode`) — its
    Cargo features (`categories`, `grapheme_clusters`) show it doesn't
    implement word-boundary segmentation at all, so no substitute exists.
  - Empirically classified every ASCII punctuation character's
    join-two-words behavior against real `unicode_words()` output (only
    `' . : , ;` ever join, and only a single occurrence between the right
    adjacent-character classes — hyphens/em-dashes never join, so
    "co-authored" is 2 words but "don't" is 1). Wrote `ascii_word_count`
    to that spec, with `count_words` falling back to `unicode_words()` for
    any line containing a non-ASCII byte (rare in prose: 6 of 3,135 lines
    in the user's real book).
  - Validated with a differential test (`ascii_word_count_matches_unicode_words`,
    40 synthetic edge cases, and `..._over_sample_fixture` against
    `resource/sample.md`) asserting equality against `unicode_words()`
    directly, so it stays correct-by-definition if that crate ever changes.
    Also spot-checked against all 2,963 real paragraph lines in the user's
    book (111,418/111,418 words matching) before committing.
  - **Net result: ~8.06ms → ~3.13ms per run on the user's real book, a
    2.57x end-to-end speedup**, confirmed via interleaved cold-process A/B
    (not a single measurement) with byte-identical table output. The
    user's call: not worth it for flow/performance on this machine, but
    worth having for portability to slower runtime environments.

Still open (lower priority per user — not yet hit in real usage):

- **Real panic, not theoretical**: `fmt/heading.rs` truncates long headings
  with `&heading[..48]`, a byte-index slice. Any heading whose 50-byte
  window splits a multi-byte UTF-8 character (em dash, curly quote,
  accented letter — all normal in prose) crashes the whole run with
  "byte index is not a char boundary." Reproduced locally with a
  47-ASCII-char + em-dash heading; confirmed panic on real output. Needs a
  char-boundary-aware truncation (e.g. `unicode-segmentation` grapheme
  truncation, or scan back from the byte cutoff to a valid boundary).
- **Heading-out-of-order tree building** (`Document::new_document` /
  `last_document`): headings are always attached via `self.root.new_document(level)`
  (root-relative, not relative to the current node), and skipping levels
  (e.g. `#` straight to `###`) synthesizes headless placeholder documents
  as phantom parents. This is intentional per the code comments ("left as
  an exercise to the reader") and real manuscripts in the test fixture
  don't currently trigger it, but it's a sharp edge if heading levels ever
  get used non-sequentially.
- No test suite exists (`grep -rn '#\[test\]' src/` is empty). Given the
  panic above was only caught by manually crafting input, some unit
  coverage on `document.rs` (tree building) and `fmt/heading.rs`
  (truncation) would have caught it for free.

## Conventions observed

- Modules are thin and single-purpose (`cli`, `document`, `filter`, `fmt`,
  `log`, `error`); `fmt::heading` is a private submodule of `fmt`.
  Keep that shape — don't collapse things back into `main.rs`.
- Errors go through `thiserror` via a single `error::Error` enum
  (currently just wraps `io::Error`); `main.rs` prints and exits 1 rather
  than panicking, except where `document.rs`/`fmt/heading.rs` still panic
  on bad input (see above) — that's a deviation from the project's own
  error-handling convention, not a deliberate design choice.
- `tracing` is opt-in via `RUST_LOG`/`LOG` env var (see `log::init`); it's
  a no-op subscriber otherwise, so `tracing::debug!` calls in the hot path
  are effectively free and fine to leave in place.
- Release profile is tuned for a small fast binary (`lto = true`,
  `codegen-units = 1`, `panic = "abort"`) — consistent with the "don't
  break flow state" goal; keep new dependencies lean for the same reason.
  `ratatui` is added with `default-features = false, features = ["crossterm"]`
  rather than its default `all-widgets` feature set, for the same reason —
  only `Table`/`Paragraph` and a crossterm backend are actually used.
- **No `mod.rs` files, ever** — explicit user preference. A module with
  submodules is always `name.rs` + `name/*.rs` (2018-style paths), e.g.
  `fmt.rs` + `fmt/heading.rs`, `tui.rs` + `tui/{app,render,watch}.rs`.
