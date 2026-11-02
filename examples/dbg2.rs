fn main() {
    let src = "enum X {\n  pending @label { zh \"P\" }\n}\n";
    let toks = forma::dsl::lexer::lex(src);
    for t in &toks {
        println!("{:?} {:?}", t.kind, t.value);
    }
}
