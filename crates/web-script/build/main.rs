mod generate;
mod idl;

use std::path::PathBuf;

fn main() -> Result<(), String> {
    let directory = PathBuf::from("webidl");
    println!("cargo::rerun-if-changed=webidl");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&directory)
        .map_err(|error| error.to_string())?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "webidl")
        })
        .collect();
    files.sort();
    let mut definitions = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
        idl::parse(&source, &mut definitions)
            .map_err(|error| format!("{}: {error}", file.display()))?;
    }
    let code = generate::generate(&definitions).map_err(|error| error.to_string())?;
    let out = PathBuf::from(std::env::var("OUT_DIR").map_err(|error| error.to_string())?);
    std::fs::write(out.join("bindings.rs"), code).map_err(|error| error.to_string())
}
