//! Owns the global-hotkey manager and the currently-registered summon shortcut,
//! so it can be re-registered live when the user changes it in settings.

use global_hotkey::hotkey::HotKey;
use global_hotkey::{Error, GlobalHotKeyManager};

pub struct Hotkey {
    manager: GlobalHotKeyManager,
    current: HotKey,
}

impl Hotkey {
    /// Creates the manager and registers `initial`. Registration failure (e.g.
    /// the combo is already taken) is logged, not fatal.
    pub fn new(initial: HotKey) -> Result<Self, Error> {
        let manager = GlobalHotKeyManager::new()?;
        if let Err(err) = manager.register(initial) {
            eprintln!("glance: failed to register shortcut {initial} ({err}).");
        }
        Ok(Self {
            manager,
            current: initial,
        })
    }

    pub fn id(&self) -> u32 {
        self.current.id()
    }

    pub fn current(&self) -> HotKey {
        self.current
    }

    /// Swaps the registered shortcut. Returns an error (leaving the old one in
    /// place) if the new combo can't be registered.
    pub fn set(&mut self, new: HotKey) -> Result<(), Error> {
        let _ = self.manager.unregister(self.current);
        self.manager.register(new)?;
        self.current = new;
        Ok(())
    }
}
