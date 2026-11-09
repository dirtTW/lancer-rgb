fn main() {
    // The Win32 shell calls user32 and gdi32 directly.
    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rustc-link-lib=gdi32");
}
