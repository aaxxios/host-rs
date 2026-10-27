use hosting::Service;

#[derive(Service)]
enum Storage {
    Memory,
    Disk,
}

fn main() {}
