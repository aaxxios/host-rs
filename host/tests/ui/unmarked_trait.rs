use host::{Service, ServiceCollection};

trait Plain: Send + Sync {}

#[derive(Service)]
struct P;

impl Plain for P {}

fn main() {
    let mut services = ServiceCollection::new();
    services.add_singleton_as::<dyn Plain, P>();
}
