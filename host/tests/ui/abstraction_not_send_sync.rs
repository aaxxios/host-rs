use host::abstraction;

#[abstraction]
trait Sender {
    fn send(&self);
}

fn main() {}
