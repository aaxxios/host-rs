use host::{Service, ServiceCollection};

#[derive(Service)]
struct Smtp;

struct Mailer;

fn main() {
    let mut services = ServiceCollection::new();
    services.add_singleton_as::<Mailer, Smtp>();
}
