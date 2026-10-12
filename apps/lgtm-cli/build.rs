use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=bundle");
    let bundle = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("bundle");
    assert!(
        bundle.join("web/index.html").exists(),
        "Missing CLI bundle. Run mise run //apps/lgtm-cli:bundle before building from source."
    );
    let out = env::var("OUT_DIR").unwrap();
    for file in fs::read_dir(bundle.join("proto")).unwrap() {
        let file = file.unwrap();
        if file.path().extension().is_some_and(|ext| ext == "rs") {
            fs::copy(file.path(), Path::new(&out).join(file.file_name())).unwrap();
        }
    }
    let mut source = String::from(
        "pub fn get(path: &str) -> Option<(&'static [u8], &'static str)> { match path {\n",
    );
    assets(&bundle.join("web"), &bundle.join("web"), &mut source);
    source.push_str("_ => None, } }\n");
    fs::write(Path::new(&out).join("assets.rs"), source).unwrap();
}

fn assets(root: &Path, dir: &Path, source: &mut String) {
    let mut files: Vec<_> = fs::read_dir(dir).unwrap().map(Result::unwrap).collect();
    files.sort_by_key(|entry| entry.file_name());
    for file in files {
        let path = file.path();
        if path.is_dir() {
            assets(root, &path, source);
        } else {
            let route = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            let mime = mime_guess::from_path(&path)
                .first_or_octet_stream()
                .to_string();
            source.push_str(&format!(
                "{route:?} => Some((include_bytes!({:?}), {mime:?})),\n",
                path.to_str().unwrap()
            ));
        }
    }
}
