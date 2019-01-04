# ncount

> Derive narrative stats for markdown files.

There is certain information we care in fiction. The number of words, the length of paragraphs, etc. Everything else is just noise. Specifically designed to work on Markdown files, excepting headings and comments from the word count. Also ignores block quotes at the moment. Kind of torn on that.

## Usage

```shell
$ ncount ./dir/
 §             Count ¶   Avg ¶   Long ¶   Words
 Chapter 1.1        43      37       94    1601
 Chapter 1.2        52      30       86    1607
 Chapter 1.3        43      33       87    1451
 Chapter 1.4        37      40       79    1492
 Chapter 1.5        43      32       83    1409
 Chapter 1.6        48      38       74    1831
 Chapter 2.1        45      32       86    1449
 Chapter 2.2        30      39       79    1196
 Chapter 2.3        40      34       79    1378
 Chapter 2.4        46      36       87    1686
 Chapter 2.5        35      35       90    1237
 Chapter 2.6        23      39       93     904
 Chapter 2.7        43      41      102    1777
 Chapter 3.1        41      32       98    1324
 Chapter 3.2         4      76       99     305
                   573      36      102   20647
```

This program no longer uses the glob crate for finding file, but rather uses walkdir to just list all the files in a given directory. The output columns present the following information:

1. Section name
2. Paragraph count
3. Average paragraph length
4. Max paragraph length
5. Word count

Each of these properties comes in handy for me in some way. Obviously, I don't care about the number of characters or bytes or whatever. The program now parses markdown using pest, but the grammar is extremely simple and intended only to detect paragraphs and ignore comments and headings.

## Desired features

Someday, in my copious free time, I intend to cause the program to display the total length of chapters. That said, this will likely only work in accordance with my particular chapter naming scheme (which is demonstrated in the above program output).
