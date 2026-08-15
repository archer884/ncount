# ncount

```shell
❯ ncount --help
A word count tool that derives useful stats from Markdown, ignoring HTML comments and footnotes

Usage: ncount [OPTIONS] [PATHS]...

Arguments:
  [PATHS]...  files, directories, or glob patterns

Options:
  -f, --filter <FILTER>  show only one heading's section (case-insensitive prefix match)
  -v, --verbose          print paragraph count, average, and longest
  -w, --watch            watch files and launch the interactive TUI
  -h, --help             Print help
  -V, --version          Print version
```

There is certain information we care in fiction. The number of words, the length of paragraphs, etc. Everything else is just noise. Specifically designed to work on Markdown files, treating `<!-- -->` HTML comments and `[^footnotes]` as noise and never counting them.

## Usage

```shell
❯ ncount .\src\ -v
 §                          Count ¶   Avg ¶   Long ¶   Words   Total
 Chapter I: Die Walküre
 1.1                             44      39      112    1723    1723
 From the Book of Shadows        11      30       76     336    2059
 1.2                             60      33      109    1993    4052
 1.3                             43      34      109    1472    5524
 1.4                             56      30      102    1725    7249
 1.5                             16      55      123     884    8133
 1.6                             41      46      105    1891   10024
 1.7                             31      40      126    1248   11272
 1.8                             59      36      132    2169   13441
 Chapter II: The Chosen
 2.1                             51      36      110    1845   15286
 2.2                             24      45      128    1091   16377
 2.3                             23      44      138    1033   17410
 2.4                             24      42      114    1008   18418
 2.5                             78      31      132    2490   20908
 2.6                             38      38      159    1473   22381
 From the Book of Shadows         8      51       120     412   22793
 2.7                             40      56      139    2244   25037
 2.8                             32      49      126    1574   26611
 Chapter III: The Prince
 3.1                             28      47      139    1321   27932
 3.2                             32      39      134    1271   29203
 3.3                              2      49       61      99   29302
 Errata                           3      19       27      57   29359
                                 744      39      159   29359
```

The `--verbose` flag causes paragraph information to be printed, including paragraph count, longest and average length, while the `--filter` flag permits the user to focus only on a given heading and its subheadings. For example:

```shell
❯ ncount .\src\ -f "iii:"
 §                         Words   Total
 Chapter III: The Prince
 3.1                        1321    1321
 3.2                        1271    2592
 3.3                          99    2691
```

## Watch mode

`ncount -w <paths>` opens an interactive table instead, watching the given files and rebuilding each one the moment you save.

Quote glob patterns so the shell passes them through: `ncount -w 'src/chapter.*'`. A quoted pattern is re-expanded live as files appear and disappear; an unquoted glob is expanded by the shell before `ncount` ever sees it.

| Key | Action |
| --- | --- |
| `j` `k` `↑` `↓` | move the selection (mouse wheel too) |
| `PgDn` `PgUp` | scroll a page |
| `l` `→` | unfold the selected section |
| `h` `←` | fold it (on a leaf, folds its parent) |
| `v` `Space` | pin/unpin — a pinned section stays visible when folded |
| `f` `/` | filter by heading (`Enter` applies, `Esc` cancels) |
| `?` | show the shortcut list |
| `q` `Esc` `Ctrl-C` | quit |

## To cross-compile for Windows:

```shell
$ cargo build --target x86_64-pc-windows-gnu --release
```

## Changelog

### 0.7.6 (2026-08-15)

- Interactive TUI with native watch mode (`-w`/`--watch`): a live stats
  table that rebuilds each file the moment you save it. Foldable heading
  tree with pinned exceptions, in-app filter input, page scrolling,
  mouse wheel support, and a `?` shortcuts dialog. Quoted glob patterns
  re-expand live as files appear and disappear. (Shipped across
  0.7.4–0.7.6.)
- Cleaned up and shortened the help text; updated to the 2024 edition
  and upgraded dependencies.

### 0.7.3 (2026-07-13)

- ASCII fast path for word counting (~2.6x faster end-to-end on a full
  book), a warning when `--filter` matches no heading, and package
  upgrades.
