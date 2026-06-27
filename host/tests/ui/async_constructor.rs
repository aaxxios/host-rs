use host::Service;

#[derive(Service)]
#[service(constructor = connect)]
struct Database;

impl Database {
    async fn connect() -> Self {
        Database
    }
}

fn main() {}
