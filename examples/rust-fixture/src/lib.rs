//! Small real Rust workspace for optional rust-analyzer integration tests.
pub trait Greeter {
    fn greet(&self, name: &str) -> String;
}

pub struct Friendly;
impl Greeter for Friendly {
    fn greet(&self, name: &str) -> String {
        greet(name)
    }
}

pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

pub fn welcome() -> String {
    greet("Odin")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn greets_odin() {
        assert_eq!(welcome(), "Hello, Odin!");
    }
}
