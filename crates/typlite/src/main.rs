//! # Typlite

use typlite::Typlite;

fn main() {
    let input = r#"..\..\ts\typst-blog\source\_posts\simple-se.typ"#;
    let input = std::fs::read_to_string(input).unwrap();
    let typlite = Typlite::new_with_content(&input);
    let output = typlite.convert().unwrap();
    std::fs::write(r#"..\..\ts\typst-blog\public\simple-se.md"#, output).unwrap();
}
