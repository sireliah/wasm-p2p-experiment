use prost_build;

fn build_proto() {
    prost_build::compile_protos(&["src/metadata.proto"], &["src/"]).unwrap();
}

fn main() {
    build_proto();
}
