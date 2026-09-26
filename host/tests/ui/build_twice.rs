use host::Host;

fn main() {
    let builder = Host::builder();
    let _first = builder.build();
    let _second = builder.build();
}
