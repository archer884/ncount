# ncount

```shell
❯ ncount --help
ncount 0.5.1
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
❯ ncount .\src\ -d
 §                          Count ¶   Avg ¶   Long ¶   Words   Total
 Chapter I: Die Walküre
 1.1                             44      39      112    1723    1723
 From the Book of Shadows        10      33       76     332    2055
 1.2                             60      33      109    1993    4048
 1.3                             43      34      109    1472    5520
 1.4                             56      30      102    1725    7245
 1.5                             16      55      123     884    8129
 1.6                             41      46      105    1891   10020
 1.7                             31      40      126    1248   11268
 1.8                             63      34      126    2147   13415
 Chapter II: The Chosen
 2.1                             57      39      117    2251   15666
 2.2                             42      35       96    1488   17154
 2.3                              7      33      111     237   17391
 Chapter III: ???
 3.1                             46      36      112    1676   19067
 3.2                             45      38      108    1727   20794
 Errata                           3      19       27      57   20851
                                564      36      126   20851
```

The `--detail` flag causes paragraph information to be printed, including paragraph count, longest and average length, while the `--filter` flag permits the user to focus only on a given heading. For example:

```shell
❯ ncount .\src\ -f "2."
 §     Words   Total
 2.1    2251    2251
 2.2    1488    3739
 2.3     237    3976
```
