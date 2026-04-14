mod manager;
mod plugin;
mod service;

pub use service::*;

mod wit {
    use wasmtime::component::bindgen;

    bindgen!({
        path: "../pdk/wit",
        world: "storefront-plugin",
        imports: { default: async },
        exports: { default: async }
    });
}
