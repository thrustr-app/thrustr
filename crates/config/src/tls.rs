use rustls::crypto::ring;

pub fn init() {
    let _ = ring::default_provider().install_default();
}
