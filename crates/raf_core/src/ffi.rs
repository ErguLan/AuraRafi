use crate::complement::{EngineComplement, ComplementDomain, ComplementPresentation, ComplementContext};
use crate::command::Command;
use libloading::Library;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::path::Path;

/// C-friendly wrapper over our CommandBus for FFI interop.
#[repr(C)]
pub struct CApiCommandBus {
    pub engine_ctx: *mut c_void,
    pub submit_json_command: unsafe extern "C" fn(*mut c_void, *const c_char),
}

/// The entire AuraRafi API exposed to C++.
#[repr(C)]
pub struct CAuraRafiAPI {
    pub command_bus: CApiCommandBus,
}

// Expected signatures inside the C++ DLL.
type FfiInitPlugin = unsafe extern "C" fn(api: *const CAuraRafiAPI) -> bool;
type FfiUpdatePlugin = unsafe extern "C" fn();
type FfiPluginName = unsafe extern "C" fn() -> *const c_char;
type FfiPluginDomain = unsafe extern "C" fn() -> i32; 

/// Wraps a dynamically loaded generic C/C++ native plugin into the Engine OS Mod ecosystem.
pub struct CppComplement {
    id: String,
    name: String,
    domain: ComplementDomain,
    presentation: ComplementPresentation,
    
    // We hold the DLL library strictly so it is not unmapped from memory during the engine lifecycle.
    _lib: Library,
    
    // Extracted raw function pointers (we guarantee _lib outlives these pointers implicitly).
    init_fn: *const (),
    update_fn: *const (),
}

impl CppComplement {
    /// Loads a `.dll` (Windows) or `.so` (Linux).
    pub fn load_dll<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        
        unsafe {
            let lib = Library::new(path.as_ref()).map_err(|e| format!("Failed to load DLL {}: {}", path_str, e))?;
            
            // Extract Name
            let name_sym: libloading::Symbol<FfiPluginName> = lib.get(b"aura_rafi_plugin_name\0")
                .map_err(|_| "Missing extern 'C' fn aura_rafi_plugin_name() in DLL")?;
            let cstr_name = CStr::from_ptr(name_sym());
            let name = cstr_name.to_string_lossy().into_owned();
            
            // Extract Domain (0 = Universal, 1 = Games, 2 = Electronics)
            let domain_sym: libloading::Symbol<FfiPluginDomain> = lib.get(b"aura_rafi_plugin_domain\0")
                .map_err(|_| "Missing extern 'C' fn aura_rafi_plugin_domain() in DLL")?;
            let domain_val = domain_sym();
            let domain = match domain_val {
                1 => ComplementDomain::Games,
                2 => ComplementDomain::Electronics,
                _ => ComplementDomain::Universal,
            };

            // Extract Init and Update
            let init_sym: libloading::Symbol<FfiInitPlugin> = lib.get(b"aura_rafi_plugin_init\0")
                .map_err(|_| "Missing extern 'C' fn aura_rafi_plugin_init(api)")?;
            let update_sym: libloading::Symbol<FfiUpdatePlugin> = lib.get(b"aura_rafi_plugin_update\0").unwrap_or_else(|_| {
                // Return a dummy symbol if not implemented, avoiding crashes.
                let dummy: libloading::Symbol<FfiUpdatePlugin> = std::mem::transmute(dummy_update as *const ());
                dummy
            });

            // Store raw pointers to satisfy lifetimes. 
            // Safety: These pointers point to instructions mapped into memory by `_lib`. `_lib` owns this memory.
            let init_fn = *init_sym as *const ();
            let update_fn = *update_sym as *const ();

            Ok(Self {
                id: format!("cpp_{}", uuid::Uuid::new_v4()),
                name,
                domain,
                presentation: ComplementPresentation::Headless, // Default for C++ till UI bridge is fully integrated
                _lib: lib,
                init_fn,
                update_fn,
            })
        }
    }
}

// Dummy handler fallback if C++ does not implement update hook.
unsafe extern "C" fn dummy_update() {}

// Callback injected into C++ DLL. It receives JSON, invokes local CommandBus safely.
unsafe extern "C" fn c_api_submit_json_command(context: *mut c_void, json_str: *const c_char) {
    if context.is_null() || json_str.is_null() { return; }
    
    let bus_ptr = context as *mut crate::command::CommandBus;
    let bus = &mut *bus_ptr;
    
    if let Ok(c_str) = CStr::from_ptr(json_str).to_str() {
        if let Ok(cmd) = serde_json::from_str::<Command>(c_str) {
            bus.submit(cmd);
        } else {
            tracing::error!("C++ Mod submitted invalid JSON Command structure: {}", c_str);
        }
    }
}

impl EngineComplement for CppComplement {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn domain(&self) -> ComplementDomain {
        self.domain
    }

    fn presentation(&self) -> ComplementPresentation {
        self.presentation
    }

    fn on_init(&mut self, context: &mut ComplementContext<'_>) {
        // Construct the immutable C API referencing our CommandBus pointer context
        let api = CAuraRafiAPI {
            command_bus: CApiCommandBus {
                engine_ctx: context.command_bus as *mut _ as *mut c_void,
                submit_json_command: c_api_submit_json_command,
            }
        };

        unsafe {
            let func: FfiInitPlugin = std::mem::transmute(self.init_fn);
            if !func(&api) {
                tracing::warn!("C++ Plugin '{}' initialization failed (returned false).", self.name);
            }
        }
    }

    fn on_update(&mut self, _context: &mut ComplementContext<'_>) {
        unsafe {
            let func: FfiUpdatePlugin = std::mem::transmute(self.update_fn);
            func();
        }
    }
    
    fn draw_ui(&mut self, _context: &mut ComplementContext<'_>) {
        // Future projection: pass rendering context. Currently C++ is Headless logic oriented.
    }
}
