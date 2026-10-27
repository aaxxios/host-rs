use hosting::abstraction;

#[abstraction]
trait Source: Send + Sync {
    type Item<T>;
}

fn main() {}
