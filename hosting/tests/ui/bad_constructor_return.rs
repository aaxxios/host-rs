use hosting::Service;

#[derive(Service)]
#[service(constructor = connect)]
struct Database;

impl Database {
    fn connect() -> Option<Self> {
        Some(Database)
    }
}

fn main() {}
