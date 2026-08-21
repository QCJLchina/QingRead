// 模拟 NSIS 安装环境：把数据目录指向 %APPDATA%\EpubReader
// 用 stdin 发送命令到 dev server? 不行，需要直接调 IPC。
// 改用：写一个测试 exe，调用 commands::library::import_book
// 但这需要改 main.rs。简单点：直接读 settings.json 看路径。

use std::path::PathBuf;

fn main() {
    let appdata = std::env::var("APPDATA").unwrap();
    let data_dir = PathBuf::from(&appdata).join("EpubReader");
    println!("APPDATA: {}", appdata);
    println!("data_dir: {}", data_dir.display());
    println!("data_dir exists: {}", data_dir.exists());

    let settings_file = data_dir.join("settings.json");
    println!("settings_file: {}", settings_file.display());
    println!("settings exists: {}", settings_file.exists());
    if settings_file.exists() {
        let content = std::fs::read_to_string(&settings_file).unwrap();
        println!("settings content:");
        println!("{}", content);
    }
    let library_file = data_dir.join("library.json");
    println!("library exists: {}", library_file.exists());
}
