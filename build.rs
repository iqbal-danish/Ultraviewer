#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("Ultraviewer.ico");
    if let Err(e) = res.compile() {
        eprintln!("Failed to compile Windows resources: {}", e);
    }
}

#[cfg(not(windows))]
fn main() {}
