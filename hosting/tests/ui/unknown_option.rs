use hosting::Service;

#[derive(Service)]
#[service(lifetime = singleton)]
struct Database;

fn main() {}
