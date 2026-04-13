mod manager;
mod plugin;

pub use manager::PluginManager;

mod wit {
    use wasmtime::component::bindgen;

    bindgen!({
        path: "../pdk/wit",
        world: "storefront-plugin",
        imports: { default: async },
        exports: { default: async }
    });
}
