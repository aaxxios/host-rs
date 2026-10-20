use std::sync::Arc;

/// Converts an implementation into the trait object it is registered as.
///
/// Implemented for `dyn Trait` by `#[abstraction]`, which lets any
/// implementation of the trait be registered with `add_*_as`. Stable Rust has
/// no generic unsizing, so each trait needs this one impl.
#[diagnostic::on_unimplemented(
    message = "`{T}` cannot be registered as `{Self}`",
    note = "`{Self}` must be a trait object whose trait has `#[abstraction]` and is implemented for `{T}`"
)]
pub trait CastFrom<T> {
    /// Upcasts `from` to the trait object.
    fn cast(from: Arc<T>) -> Arc<Self>;
}
