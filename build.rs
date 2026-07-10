fn main() {
    #[cfg(windows)]
    let _ = embed_resource::compile("app.rc", embed_resource::NONE);
}
