use crate::{app::App, control, persistence::Persistence, session};
use serde_json::Value;
use std::{error::Error, fs::File, path::PathBuf};

pub struct GuiSession {
    name: String,
    _lock: File,
    persistence: Persistence,
    app: App,
}

impl GuiSession {
    pub fn open(name: &str, executable: PathBuf, cwd: PathBuf) -> Result<Self, Box<dyn Error>> {
        session::validate_name(name)?;
        let directory = session::root()?;
        let lock = session::private_file(&directory.join(format!("{name}.lock")), false)?;
        lock.try_lock()
            .map_err(|_| "Muxer session is already running")?;
        let (mut persistence, saved) = Persistence::open(&directory, name);
        let mut app = if let Some(snapshot) = saved.filter(|snapshot| !snapshot.is_empty()) {
            App::restore(executable, snapshot)?
        } else {
            let mut app = App::new(executable)?;
            app.add_space(cwd)?;
            app.select_space(0);
            app
        };
        app.persistent = true;
        app.resize_headless()?;
        persistence.save(&app);
        Ok(Self {
            name: name.into(),
            _lock: lock,
            persistence,
            app,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn refresh(&mut self) -> Result<Value, String> {
        let _ = self.app.config.refresh(false);
        for pane in &mut self.app.panes {
            pane.poll().map_err(|error| error.to_string())?;
        }
        self.app
            .resize_headless()
            .map_err(|error| error.to_string())?;
        self.persistence.save(&self.app);
        Ok(self.app.automation_snapshot())
    }

    pub fn request(&mut self, request: Value) -> Result<Value, String> {
        let request: control::Request =
            serde_json::from_value(request).map_err(|error| error.to_string())?;
        let result = self
            .app
            .automation(&request)
            .map_err(|error| error.to_string())?;
        self.persistence.save(&self.app);
        Ok(result)
    }
}
