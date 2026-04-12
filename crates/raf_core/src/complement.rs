use serde::{Deserialize, Serialize};

/// The domain where the complement should be active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplementDomain {
    /// Only active for Video Game projects
    Games,
    /// Only active for Electronics/Circuit projects
    Electronics,
    /// Active everywhere
    Universal,
}

/// How the complement will present itself in the Engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplementPresentation {
    /// Placed alongside Console, AI Chat, etc.
    BottomTab,
    /// A standalone floating pop-up window
    FloatingWindow,
    /// No UI. Runs quietly in the background on every tick.
    Headless,
}

/// The context passed to complements so they can mutate the engine state.
/// This will eventually wrap the AI Tool Registry (e.g. `create_entity`, `place_component`),
/// providing an API that is completely safe and mirrored to the AI's capabilities.
pub struct ComplementContext<'a> {
    pub lang: crate::Language,
    pub command_bus: &'a mut crate::command::CommandBus,
}

/// The main Trait that all Open Source mods/extensions must implement.
pub trait EngineComplement {
    /// Unique identifier for the complement
    fn id(&self) -> &str;
    
    /// Display name (for tabs and windows)
    fn name(&self) -> &str;
    
    /// Target domain (Games, Electronics, Universal)
    fn domain(&self) -> ComplementDomain;
    
    /// How this complement shows up in the UI
    fn presentation(&self) -> ComplementPresentation;
    
    /// Called once when the complement is registered
    fn on_init(&mut self, _context: &mut ComplementContext<'_>) {}
    
    /// Called every frame if Headless or if window is open
    fn on_update(&mut self, _context: &mut ComplementContext<'_>) {}
    
    /// The actual egui code. Only called if Presentation is BottomTab or FloatingWindow.
    /// The context is where we eventually bind UI frameworks (like egui).
    /// Kept out of raf_core to preserve separation of concerns. To draw we pass raw UI downcasted via Any or safely bridged later.
    fn draw_ui(&mut self, _context: &mut ComplementContext<'_>) {}
}

/// The registry that holds all active Complements.
pub struct ComplementRegistry {
    pub complements: Vec<Box<dyn EngineComplement>>,
}

impl ComplementRegistry {
    pub fn new() -> Self {
        Self {
            complements: Vec::new(),
        }
    }

    /// Open Source developers will call this to inject their structs.
    pub fn register(&mut self, complement: Box<dyn EngineComplement>) {
        self.complements.push(complement);
    }
}
