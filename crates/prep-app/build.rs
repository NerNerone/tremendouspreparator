//! Compila la UI Slint (stile material). Se nella radice del progetto c'è una cartella
//! `content/` (e `books/`), la include nell'app come esame predefinito: serve a chi vuole
//! distribuire una build con contenuti propri; la build pubblica non ne ha.
use std::fmt::Write;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, exts, out);
            } else if p.extension().is_some_and(|x| exts.iter().any(|e| x == *e)) {
                out.push(p);
            }
        }
    }
}

fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_style("material".into())
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);
    slint_build::compile_with_config("ui/app.slint", config).expect("compilazione UI");

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = manifest.parent().unwrap().parent().unwrap().to_path_buf();
    let content = repo.join("content");
    println!("cargo:rerun-if-changed={}", content.display());
    let mut files = Vec::new();
    walk(&content, &["json"], &mut files);
    files.sort();
    let mut src = String::from("pub static BUNDLED: &[(&str, &str)] = &[\n");
    for f in files {
        println!("cargo:rerun-if-changed={}", f.display());
        let rel = f.strip_prefix(&repo).unwrap().to_string_lossy().replace('\\', "/");
        writeln!(src, "    ({:?}, include_str!({:?})),", rel, f.display().to_string()).unwrap();
    }
    src.push_str("];\n");

    // libri (markdown + formule SVG): l'app funziona anche senza sincronizzazione
    let books = repo.join("books");
    println!("cargo:rerun-if-changed={}", books.display());
    let mut files = Vec::new();
    walk(&books, &["md", "svg"], &mut files);
    files.sort();
    src.push_str("pub static BUNDLED_BOOKS: &[(&str, &[u8])] = &[\n");
    for f in files {
        let rel = f.strip_prefix(&repo).unwrap().to_string_lossy().replace('\\', "/");
        writeln!(src, "    ({:?}, include_bytes!({:?})),", rel, f.display().to_string()).unwrap();
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("bundled.rs");
    std::fs::write(out, src).unwrap();

    // versione mostrata e scritta nelle sessioni: quella di Cargo.toml, più l'identificativo della
    // build se la CI lo passa (anteprime). Senza APP_BUILD la build è uguale da chiunque venga fatta.
    let pkg = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let version = match std::env::var("APP_BUILD") {
        Ok(b) if !b.trim().is_empty() => format!("{pkg}+{}", b.trim()),
        _ => pkg,
    };
    println!("cargo:rerun-if-env-changed=APP_BUILD");
    println!("cargo:rustc-env=APP_VERSION={version}");
}
