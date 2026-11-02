use anyhow::Result;
fn main() -> Result<()> {
    let src = std::fs::read_to_string("../../examples/school/forma/app.fm")?;
    let toks = forma::dsl::lexer::lex(&src);
    let out = forma::dsl::parser::parse(&toks);
    for p in &out.program.pages {
        if let forma::dsl::PageBody::Form(f) = &p.body {
            println!(
                "{}: {:?}",
                p.name,
                f.fields
                    .iter()
                    .map(|x| (x.path.clone(), x.widget.clone(), x.from.clone(), x.required))
                    .collect::<Vec<_>>()
            );
            println!("   validation: {:?}", f.validation);
        }
    }
    let _ = Result::<()>::Ok(());
    Ok(())
}
