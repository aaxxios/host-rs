use hosting::abstraction;

#[abstraction]
trait Sender<'a>: Send + Sync {
    fn send(&self, message: &'a str);
}

fn main() {}
