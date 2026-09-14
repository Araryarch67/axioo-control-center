//! Copy ikon Lucide ke OUT_DIR agar binary menemukannya saat runtime
//! tanpa peduli CWD (`/usr/share/...` dipakai bila terinstal sistem).

fn main() {
    println!("cargo:rerun-if-changed=assets/icons/");
    let out = std::env::var("OUT_DIR").expect("OUT_DIR from cargo");
    let dst = std::path::Path::new(&out).join("icons");
    std::fs::create_dir_all(&dst).expect("create icons OUT_DIR");
    let mut n = 0;
    if let Ok(entries) = std::fs::read_dir("assets/icons") {
        for e in entries.flatten() {
            if e.path().extension().and_then(|x| x.to_str()) == Some("svg") {
                std::fs::copy(e.path(), dst.join(e.file_name())).expect("copy icon");
                n += 1;
            }
        }
    }
    println!("cargo:warning=axioo-gui icons bundled: {n}");
}
