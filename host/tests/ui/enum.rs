use host::Service;

#[derive(Service)]
enum Storage {
    Memory,
    Disk,
}

fn main() {}
