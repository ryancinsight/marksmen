use anyhow::Result;
use wasmtime::*;

pub struct PluginEngine {
    engine: Engine,
    module: Module,
}

impl PluginEngine {
    /// Initialize the Plugin Engine with a compiled WebAssembly module
    pub fn new(wasm_bytes: &[u8]) -> Result<Self> {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm_bytes)?;
        Ok(Self { engine, module })
    }

    /// Passes the JSON AST to the WebAssembly guest, executes the filter,
    /// and returns the mutated JSON AST.
    pub fn apply_filter(&self, json_payload: &str) -> Result<String> {
        let mut store = Store::new(&self.engine, ());

        let instance = Instance::new(&mut store, &self.module, &[])?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("failed to find `memory` export"))?;

        let filter_func =
            instance.get_typed_func::<(u32, u32), (u32, u32)>(&mut store, "filter")?;

        let alloc_func = instance
            .get_typed_func::<u32, u32>(&mut store, "alloc")
            .unwrap_or_else(|_| panic!("Guest must export `alloc(size: u32) -> ptr`"));

        let input_len = json_payload.len() as u32;
        let input_ptr = alloc_func.call(&mut store, input_len)?;

        memory.write(&mut store, input_ptr as usize, json_payload.as_bytes())?;
        let (out_ptr, out_len) = filter_func.call(&mut store, (input_ptr, input_len))?;

        let mut out_buffer = vec![0u8; out_len as usize];
        memory.read(&mut store, out_ptr as usize, &mut out_buffer)?;

        let out_json = String::from_utf8(out_buffer)?;

        Ok(out_json)
    }
}
