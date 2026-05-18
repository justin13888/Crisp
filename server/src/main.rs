//! Crisp server — lightweight backend that stores recipe bundles and
//! indexes their metadata for fast search.

/// Returns the server's startup banner.
fn greeting() -> &'static str {
    "Crisp server ready"
}

fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("{}", greeting());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_is_ready_banner() {
        assert_eq!(greeting(), "Crisp server ready");
    }
}
