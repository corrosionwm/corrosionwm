// main.rs

fn main() {
    // link the EGL library
    println!("cargo:rustc-link-lib=dylib=EGL");

    // if the os is windows, say "why the fuck are you using windows"
    if cfg!(windows) {
        panic!("why are you trying to compile this on windows")
    }
}
