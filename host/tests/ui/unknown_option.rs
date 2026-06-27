use host::Service;

#[derive(Service)]
#[service(lifetime = singleton)]
struct Database;

fn main() {}
