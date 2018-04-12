# ncount

> Derive narrative stats for markdown files.

There is certain information we care in fiction. The number of words, the length of paragraphs, etc. Everything else is just noise. Specifically designed to work on Markdown files, excepting headings and comments from the word count. Also ignores block quotes at the moment. Kind of torn on that.

## Usage

```shell
$ ncount dir/*
Chapter 1.1  1401   43   32  94
Chapter 1.2  1441   38   37  90
Chapter 1.3  1373   33   41  84
Chapter 1.4  1136   35   32  87
Chapter 2.1  1228   43   28  74
Chapter 2.2  1437   40   35 102
Chapter 2.3  1276   38   33 104
Chapter 2.4   468   14   33  76

Total words: 9760
```

Because we're using the `glob` crate, the behavior of the glob there is the same between Windows and macOS. The output columns present the following information:

1. Section name
2. Word count
3. Paragraph count
4. Average paragraph length
5. Max paragraph length

Each of these properties comes in handy for me in some way. Obviously, I don't care about the number of characters or bytes or whatever.
