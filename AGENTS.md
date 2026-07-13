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

1. `cli::Args` — clap-parsed paths/glob patterns, `--filter`, `--verbose`.
   `materialize_files()` resolves paths/globs to files and **sorts** them,
   because non-Windows readdir order is inode order, not name order.
2. `filter::TextFilter` — one compiled regex strips comments/footnotes from
   each file's raw text before anything else sees it.
3. `document::DocumentBuilder` — walks the filtered text line by line,
   building a tree of `Document`s keyed by Markdown heading level (`#`
   depth). Each leading blank-trimmed non-heading line is a "paragraph";
   its word count comes from `unicode_words().count()`.
4. `fmt::StatFmt` — walks the finished `Document` tree, optionally filters
   to a subtree by heading (case-insensitive prefix match), and renders a
   `prettytable` table to stdout, with a running cumulative word total.

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
