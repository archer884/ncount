use ncount::DocumentStats;

fn main() {
    let content = include_str!("../resources/formatting.md");
    let mut document = DocumentStats::new();
    document.apply_str("formatting.md", content);
    println!("{}", document.as_table(true));
}
