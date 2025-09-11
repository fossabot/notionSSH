fn main() {
    // Enable UTF-8 output on Windows consoles before any printing.
    notionSSH::util::enable_windows_utf8();
    if let Err(e) = notionSSH::run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
