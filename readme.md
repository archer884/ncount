# ncount

```shell
❯ ncount --help
ncount 0.5.5
A word count program

USAGE:
    ncount.exe [FLAGS] [OPTIONS] [paths]...

FLAGS:
    -d, --detail     Print detailed document information
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v, --verbose    Print detailed document information (alias for detail)

OPTIONS:
    -f, --filter <heading>    Filter results by heading

ARGS:
    <paths>...    Paths (or globs) to be read
```

There is certain information we care in fiction. The number of words, the length of paragraphs, etc. Everything else is just noise. Specifically designed to work on Markdown files, excepting headings and comments from the word count.

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
 From the Book of Shadows         8      51      120     412   22793
 2.7                             40      56      139    2244   25037
 2.8                             32      49      126    1574   26611
 Chapter III: The Prince
 3.1                             28      47      139    1321   27932
 3.2                             32      39      134    1271   29203
 3.3                              2      49       61      99   29302
 Errata                           3      19       27      57   29359
                                744      39      159   29359
```

The `--detail`/`--verbose` flag causes paragraph information to be printed, including paragraph count, longest and average length, while the `--filter` flag permits the user to focus only on a given heading and its subheadings. For example:

```shell
❯ ncount .\src\ -f "iii:"
 §                         Words   Total
 Chapter III: The Prince
 3.1                        1321    1321
 3.2                        1271    2592
 3.3                          99    2691
```
