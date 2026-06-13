pub fn init() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to set default TLS provider");
}
