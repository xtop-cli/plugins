//! wasmi wrapper around one guest widget module.
//!
//! The guest ABI is documented in `xtop-wasm-guest`: `alloc`, `dealloc`,
//! `manifest`, `render` and `result_len`. The host writes the state JSON into
//! guest memory, calls `render`, and reads the draw-list JSON back. Guests
//! run with a memory cap and a per-call fuel budget, so a buggy or hostile
//! module cannot hang or exhaust the kernel.

use wasmi::{
    Caller, Config, Engine, Extern, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc,
};

/// Linear-memory cap per guest instance (64 MiB).
const MEMORY_LIMIT: usize = 64 * 1024 * 1024;

/// Fuel budget for one host→guest call sequence. wasmi charges fuel per
/// executed instruction; 100M covers a release-built JSON parse + render of a
/// large snapshot with a wide margin while still bounding runaway loops.
const FUEL_PER_CALL: u64 = 100_000_000;

/// Store data shared with host functions.
pub(crate) struct HostData {
    pub(crate) name: String,
    limits: StoreLimits,
}

/// A loaded, instantiated guest module.
pub(crate) struct Guest {
    store: Store<HostData>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    dealloc: TypedFunc<(i32, i32), ()>,
    manifest_fn: TypedFunc<(), i32>,
    render_fn: TypedFunc<(i32, i32), i32>,
    result_len: TypedFunc<(), i32>,
    fuel: u64,
}

impl std::fmt::Debug for Guest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guest").field("fuel", &self.fuel).finish()
    }
}

impl Guest {
    /// Compile and instantiate a module. `name` is only used for log lines.
    pub(crate) fn load(bytes: &[u8], name: &str) -> Result<Self, String> {
        Self::load_with_fuel(bytes, name, FUEL_PER_CALL)
    }

    /// Same as [`Guest::load`] with an explicit fuel budget (tests).
    pub(crate) fn load_with_fuel(bytes: &[u8], name: &str, fuel: u64) -> Result<Self, String> {
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, bytes).map_err(|e| format!("wasm compile error: {e}"))?;

        let mut store = Store::new(
            &engine,
            HostData {
                name: name.to_string(),
                limits: StoreLimitsBuilder::new().memory_size(MEMORY_LIMIT).build(),
            },
        );
        store.limiter(|data| &mut data.limits);

        let mut linker = Linker::new(&engine);
        let _ = linker.func_wrap(
            "host",
            "log",
            |caller: Caller<'_, HostData>, level: i32, ptr: i32, len: i32| {
                if ptr <= 0 || len <= 0 {
                    return;
                }
                let mut buf = vec![0u8; len as usize];
                let message = match caller.get_export("memory") {
                    Some(Extern::Memory(memory)) => {
                        match memory.read(&caller, ptr as usize, &mut buf) {
                            Ok(()) => String::from_utf8_lossy(&buf).into_owned(),
                            Err(_) => return,
                        }
                    }
                    _ => return,
                };
                let level = match level {
                    0 => "debug",
                    1 => "info",
                    2 => "warn",
                    _ => "error",
                };
                eprintln!("[wasm:{}] {level}: {message}", caller.data().name);
            },
        );

        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|e| format!("wasm instantiate error: {e}"))?;

        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| "guest does not export `memory`".to_string())?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&store, "alloc")
            .map_err(|e| format!("missing export `alloc`: {e}"))?;
        let dealloc = instance
            .get_typed_func::<(i32, i32), ()>(&store, "dealloc")
            .map_err(|e| format!("missing export `dealloc`: {e}"))?;
        let manifest_fn = instance
            .get_typed_func::<(), i32>(&store, "manifest")
            .map_err(|e| format!("missing export `manifest`: {e}"))?;
        let render_fn = instance
            .get_typed_func::<(i32, i32), i32>(&store, "render")
            .map_err(|e| format!("missing export `render`: {e}"))?;
        let result_len = instance
            .get_typed_func::<(), i32>(&store, "result_len")
            .map_err(|e| format!("missing export `result_len`: {e}"))?;

        Ok(Self {
            store,
            memory,
            alloc,
            dealloc,
            manifest_fn,
            render_fn,
            result_len,
            fuel,
        })
    }

    /// Ask the guest for its manifest.
    pub(crate) fn call_manifest(&mut self) -> Result<xtop_wasm_contract::Manifest, String> {
        self.refuel()?;
        let ptr = self
            .manifest_fn
            .call(&mut self.store, ())
            .map_err(|e| format!("manifest trap: {e}"))?;
        let bytes = self.read_result(ptr)?;
        serde_json::from_slice(&bytes).map_err(|e| format!("manifest is not valid JSON: {e}"))
    }

    /// Send the state JSON and read back the draw list.
    pub(crate) fn call_render(
        &mut self,
        state: &xtop_wasm_contract::State,
    ) -> Result<xtop_wasm_contract::DrawList, String> {
        let json = serde_json::to_vec(state).map_err(|e| format!("state encode error: {e}"))?;
        let len = json.len() as i32;
        if len <= 0 {
            return Err("empty state payload".to_string());
        }

        self.refuel()?;
        let ptr = self
            .alloc
            .call(&mut self.store, len)
            .map_err(|e| format!("alloc trap: {e}"))?;
        if ptr == 0 {
            return Err("guest alloc returned null".to_string());
        }
        self.memory
            .write(&mut self.store, ptr as usize, &json)
            .map_err(|e| format!("state write error: {e}"))?;
        let status = self
            .render_fn
            .call(&mut self.store, (ptr, len))
            .map_err(|e| format!("render trap: {e}"))?;
        let _ = self.dealloc.call(&mut self.store, (ptr, len));
        if status == 0 {
            return Err("guest rejected the state payload".to_string());
        }
        let bytes = self.read_result(status)?;
        serde_json::from_slice(&bytes).map_err(|e| format!("draw list is not valid JSON: {e}"))
    }

    fn refuel(&mut self) -> Result<(), String> {
        self.store
            .set_fuel(self.fuel)
            .map_err(|e| format!("fuel setup error: {e}"))
    }

    /// Read `result_len()` bytes starting at `ptr`.
    fn read_result(&mut self, ptr: i32) -> Result<Vec<u8>, String> {
        if ptr == 0 {
            return Err("guest returned a null pointer".to_string());
        }
        let len = self
            .result_len
            .call(&mut self.store, ())
            .map_err(|e| format!("result_len trap: {e}"))?;
        if len <= 0 {
            return Err("guest returned an empty result".to_string());
        }
        let mut buf = vec![0u8; len as usize];
        self.memory
            .read(&self.store, ptr as usize, &mut buf)
            .map_err(|e| format!("result read error: {e}"))?;
        Ok(buf)
    }
}
