fn main() {
    let v3 = "app demo { version = \"0.1.0\" }\nentity Student {\n  id Uuid @id @default(uuid)\n  name String(50) @required\n  index [name]\n}";
    let toks = forma::dsl::lexer::lex(v3);
    let out = forma::dsl::parser::parse(&toks);
    for e in &out.program.entities {
        println!("entity {}", e.name);
        for f in &e.fields {
            println!("  field {}", f.name);
        }
        println!("  indexes: {:?}", e.indexes);
    }
}
