fn main() {
    if let Err(e) = notionSSH::run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
