use host::Service;

#[derive(Service)]
#[service(constructor = connect)]
struct Database;

fn main() {}
