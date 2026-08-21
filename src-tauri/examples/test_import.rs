use std::path::PathBuf;

fn main() {
    let appdata = std::env::var("APPDATA").unwrap();
    let data_dir = PathBuf::from(&appdata).join("EpubReader");
    println!("data_dir: {}", data_dir.display());

    // Step 1: ensure_dirs equivalent
    std::fs::create_dir_all(data_dir.join("books")).unwrap();
    std::fs::create_dir_all(data_dir.join("covers")).unwrap();
    std::fs::create_dir_all(data_dir.join("progress")).unwrap();
    println!("[1] subdirs created");

    // Step 2: write library.json
    let library_file = data_dir.join("library.json");
    let entry = serde_json::json!([{
        "id": "test-uuid",
        "title": "Test Book",
        "author": "Tester",
        "cover": null,
        "file_path": "C:\\fake\\book.epub",
        "added_at": 1234567890u64,
        "file_size": 100u64,
        "format": "epub"
    }]);
    match std::fs::write(&library_file, serde_json::to_string_pretty(&entry).unwrap()) {
        Ok(()) => println!("[2] library.json written"),
        Err(e) => println!("[2] library.json FAILED: {}", e),
    }

    // Step 3: write settings.json
    let settings_file = data_dir.join("settings.json");
    let settings = serde_json::json!({
        "theme": "light",
        "font_size": 18.0,
        "line_height": 1.8,
        "font_family": "system-ui",
        "custom_bg_image": null,
        "data_dir": null,
        "close_behavior": "quit"
    });
    match std::fs::write(&settings_file, serde_json::to_string_pretty(&settings).unwrap()) {
        Ok(()) => println!("[3] settings.json written"),
        Err(e) => println!("[3] settings.json FAILED: {}", e),
    }

    println!("[4] all files:");
    for f in ["library.json", "settings.json", "books", "covers", "progress"] {
        let p = data_dir.join(f);
        println!("    {} exists={} size={}", f, p.exists(),
            if p.is_file() { p.metadata().map(|m| m.len()).unwrap_or(0) } else { 0 });
    }
}
