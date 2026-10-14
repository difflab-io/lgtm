//! A minimal Rust library generated from the `lgtm-db` template.

/// Returns the premise greeting.
pub fn hello_premise() -> &'static str {
    "hello premise"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_premise_returns_greeting() {
        assert_eq!(hello_premise(), "hello premise");
    }
}
