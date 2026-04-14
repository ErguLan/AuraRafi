# AuraRafi C++ FFI: Native Modding Guide

Welcome to the native modding documentation for AuraRafi. This guide explains how to write performant game logic, custom nodes, and engine extensions using native C++ code via Dynamic Link Libraries (`.dll` on Windows, `.so` on Linux).

## Why C++ Native Plugins?
AuraRafi is built natively in Rust. To provide maximum performance and the lowest latency possible without compromising the engine's memory safety, we use an `extern "C"` ABI bridge. This allows C++ plugins to be hot-loaded dynamically. Your C++ plugin runs natively at full speed, and talks to the engine through a strictly defined Universal Command Bus via JSON.

## The C++ API Surface
To create a script or mod, your C++ compiler simply needs to export a few expected functions and use the `CAuraRafiAPI` bridge to execute actions inside the world.

### 1. Creating your Header File (`AuraRafi.h`)
Save this header in your C++ project. This header perfectly maps to our internal Rust FFI architecture.

```cpp
#pragma once
#include <stdint.h>

extern "C" {
    // Represents the Engine's universal action queue.
    struct CApiCommandBus {
        void* engine_ctx;
        // Function pointer to send commands down to Rust's ECS
        void (*submit_json_command)(void* ctx, const char* json_str);
    };

    // The Global API state
    struct CAuraRafiAPI {
        CApiCommandBus command_bus;
    };

    // -------------------------------------------------------------
    // EXPORTED FUNCTIONS: Your C++ code MUST implement these
    // -------------------------------------------------------------

    // Returns a C-String with the name of your Plugin/Mod
    __declspec(dllexport) const char* aura_rafi_plugin_name();
    
    // Returns your UI Domain:
    // 0 = Universal (Appears Everywhere)
    // 1 = Games Domain
    // 2 = Electronics Domain
    __declspec(dllexport) int32_t aura_rafi_plugin_domain();
    
    // Called when your DLL is loaded by the engine. You should save
    // the API pointer globally so you can use it later.
    __declspec(dllexport) bool aura_rafi_plugin_init(const CAuraRafiAPI* api);
    
    // Called every engine tick (frame). Perfect for game loops!
    __declspec(dllexport) void aura_rafi_plugin_update();
}
```

### 2. Implementing the Logic (`Mod.cpp`)
Here is a complete, minimal example of a C++ Plugin for AuraRafi.

```cpp
#include "AuraRafi.h"
#include <iostream>
#include <string>

// Keep a global copy of the engine's API context
const CAuraRafiAPI* g_EngineAPI = nullptr;

// Internal frame counter
int frame_count = 0;

extern "C" {
    __declspec(dllexport) const char* aura_rafi_plugin_name() {
        return "My Epic C++ Mod";
    }

    __declspec(dllexport) int32_t aura_rafi_plugin_domain() {
        return 1; // Game Domain
    }

    __declspec(dllexport) bool aura_rafi_plugin_init(const CAuraRafiAPI* api) {
        if (!api) return false;
        
        g_EngineAPI = api;
        std::cout << "[C++] Plugin Initialized Successfully!\n";
        
        // Spawn an entity using JSON immediately on boot
        const char* spawn_cmd = R"({"type": "SpawnEntity", "name": "Player1"})";
        g_EngineAPI->command_bus.submit_json_command(g_EngineAPI->command_bus.engine_ctx, spawn_cmd);
        
        return true;
    }

    __declspec(dllexport) void aura_rafi_plugin_update() {
        frame_count++;
        
        // Example: Move the player every 60 frames
        if (frame_count % 60 == 0) {
            // Using the object's name or UUID to update its position!
            const char* move_cmd = R"({
                "type": "UpdateComponent", 
                "target": "Player1", 
                "component": "Transform", 
                "data": {"x": 10.0, "y": 0.0, "z": 0.0}
            })";
            
            g_EngineAPI->command_bus.submit_json_command(
                g_EngineAPI->command_bus.engine_ctx, 
                move_cmd
            );
        }
    }
}
```

## How to Compile
You must compile this as a Dynamic Library (DLL).
If using `g++` (MinGW) or GCC:
```bash
g++ -shared -o my_mod.dll Mod.cpp
```
If using MSVC (Visual Studio):
Ensure your project configuration type is set to **Dynamic Library (.dll)**.

## How do you control the Engine?
AuraRafi employs a **Universal Command Bus**. 
Your C++ code **does not need to know the raw Rust memory structures**. You only need to know the **Target Name or UUID** of the object in the hierarchy. By submitting a JSON string representing the command via the `submit_json_command` callback, you can:
- Construct new primitives (`SpawnEntity`).
- Move objects (`UpdateComponent` -> `Transform`).
- Switch animation states.
- Delete nodes (`DestroyEntity`).

Because the bridge relies on text/JSON passing to dispatch operations, you never have to worry about crashing the Rust ECS memory segment. Rust will safely validate your JSON command and execute it. 
