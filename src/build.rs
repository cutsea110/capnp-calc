fn main() {
    ::capnpc::CompilerCommand::new()
        .file("src/calculator.capnp")
        .run()
        .unwrap();
}
