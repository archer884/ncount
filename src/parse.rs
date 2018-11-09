use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "../resources/markdown.pest"]
pub struct MarkdownParser;
