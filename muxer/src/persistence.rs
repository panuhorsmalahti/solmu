use crate::{
    app::{App, Snapshot},
    session::private_file,
};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub struct Persistence {
    path: PathBuf,
    log: PathBuf,
    name: String,
    saved: Vec<u8>,
    backup_required: bool,
    first_save: bool,
    last_error: String,
}
impl Persistence {
    pub fn open(directory: &Path, name: &str) -> (Self, Option<Snapshot>) {
        let path = directory.join(format!("{name}.json"));
        let mut store = Self {
            path,
            log: directory.join(format!("{name}.log")),
            name: name.into(),
            saved: vec![],
            backup_required: false,
            first_save: true,
            last_error: String::new(),
        };
        let snapshot = match store.read() {
            Ok(snapshot) => snapshot,
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                store.backup_required = true;
                store.report(&format!("Could not restore the saved session: {error}"));
                None
            }
        };
        (store, snapshot)
    }
    fn read(&self) -> io::Result<Option<Snapshot>> {
        if fs::metadata(&self.path)?.len() > 1024 * 1024 {
            return Err(io::Error::other("Saved session is too large"));
        }
        let snapshot: Snapshot = serde_json::from_slice(&fs::read(&self.path)?)?;
        snapshot
            .validate()
            .map_err(|error| io::Error::other(error.to_string()))?;
        Ok(Some(snapshot))
    }
    fn report(&mut self, error: &str) {
        if error == self.last_error {
            return;
        }
        self.last_error = error.into();
        if let Ok(mut log) = private_file(&self.log, true) {
            let _ = writeln!(log, "{error}");
        }
    }
    fn backup(&mut self) -> io::Result<()> {
        let directory = self.path.parent().unwrap().join("backups");
        fs::create_dir_all(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        }
        let path = directory.join(format!(
            "{}-recovery-{}.json",
            self.name,
            uuid::Uuid::new_v4()
        ));
        let file = private_file(&path, false)?;
        fs::copy(&self.path, &path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        file.sync_all()?;
        self.report(&format!(
            "Preserved the original session at {}",
            path.display()
        ));
        let prefix = format!("{}-recovery-", self.name);
        let mut backups: Vec<_> = fs::read_dir(&directory)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
            .filter_map(|entry| {
                entry
                    .metadata()
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .map(|modified| (modified, entry.path()))
            })
            .collect();
        backups.sort_by_key(|(modified, _)| *modified);
        let excess = backups.len().saturating_sub(3);
        for (_, old) in backups.into_iter().take(excess) {
            let _ = fs::remove_file(old);
        }
        Ok(())
    }
    fn write(&mut self, app: &App) -> io::Result<()> {
        // Recheck a file that appeared between startup and the first save.
        if self.first_save && !self.backup_required && self.path.exists() && self.read().is_err() {
            self.backup_required = true;
        }
        self.first_save = false;
        if self.backup_required {
            self.backup()?;
            self.backup_required = false;
        }
        let bytes = serde_json::to_vec_pretty(&app.snapshot())?;
        if bytes == self.saved {
            return Ok(());
        }
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = private_file(&temporary, false)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            #[cfg(windows)]
            {
                crate::windows::replace(&temporary, &self.path)?;
            }
            #[cfg(unix)]
            {
                fs::rename(&temporary, &self.path)?;
                fs::File::open(self.path.parent().unwrap())?.sync_all()?;
            }
            Ok::<_, io::Error>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result?;
        self.saved = bytes;
        self.last_error.clear();
        Ok(())
    }
    pub fn save(&mut self, app: &App) {
        if let Err(error) = self.write(app) {
            self.report(&format!(
                "Could not save the session; existing state was preserved: {error}"
            ));
        }
    }
}
