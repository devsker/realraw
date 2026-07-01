fn main() {
    #[cfg(windows)]
    embed_resource::compile("app.rc");
}
