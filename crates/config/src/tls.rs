use rustls::crypto::{CryptoProvider, ring};

pub fn init() {
    if CryptoProvider::get_default().is_none() {
        let _ = ring::default_provider().install_default();
    }
}
