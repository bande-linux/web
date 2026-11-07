mod generate;
mod idl;

use std::path::PathBuf;

fn main() -> Result<(), String> {
    let directory = PathBuf::from("webidl");
    println!("cargo::rerun-if-changed=webidl");
    println!("cargo::rerun-if-changed=build");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&directory)
        .map_err(|error| error.to_string())?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "webidl")
        })
        .collect();
    files.sort();
    let mut parsed = idl::Parsed::new();
    for file in &files {
        let source = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
        parsed
            .parse(&source)
            .map_err(|error| format!("{}: {error}", file.display()))?;
    }
    let definitions = parsed.finish().map_err(|error| error.to_string())?;
    let code = generate::generate(&definitions).map_err(|error| error.to_string())?;
    let out = PathBuf::from(std::env::var("OUT_DIR").map_err(|error| error.to_string())?);
    std::fs::write(out.join("bindings.rs"), code).map_err(|error| error.to_string())
}
