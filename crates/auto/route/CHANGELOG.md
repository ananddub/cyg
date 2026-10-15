# Changelog

## 0.2.0

- `#[controller]` now automatically registers impl controllers as auto-di singletons.
- Controller impls no longer need a separate `#[singleton]` attribute.
- Controller handlers returning `impl IntoResponse` work correctly with Rust 2024 lifetime capture rules.

## 0.1.0

- Initial release with impl, standalone function, and inline module routes.
