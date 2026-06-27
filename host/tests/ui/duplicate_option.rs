use host::Service;

#[derive(Service)]
#[service(constructor = new, constructor = open)]
struct Database;

impl Database {
    fn new() -> Self {
        Database
    }

    fn open() -> Self {
        Database
    }
}

fn main() {}
