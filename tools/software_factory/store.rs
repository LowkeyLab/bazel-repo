//! Protected receipts. Opening never initializes or reconstructs missing state.
use std::{
    collections::BTreeMap,
    fs::{self, DirBuilder, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    Result,
    history::{Anchor, PendingRecord},
    model::{Action, Active, AppIdentity, Identity, IssueKey},
    require,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingEffect {
    pub action: Action,
    pub attempted: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OwnerChange {
    pub previous: Identity,
    pub owner: Identity,
    pub evidence: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub version: u8,
    pub installation: Identity,
    pub app: AppIdentity,
    pub owner: Identity,
    pub handoffs: Vec<OwnerChange>,
    #[serde(deserialize_with = "required_option")]
    pub active: Option<Active>,
    #[serde(deserialize_with = "required_option")]
    pub integrity_stop: Option<String>,
    pub anchors: BTreeMap<IssueKey, Vec<Anchor>>,
    #[serde(deserialize_with = "required_option")]
    pub pending_record: Option<PendingRecord>,
    #[serde(deserialize_with = "required_option")]
    pub pending_effect: Option<PendingEffect>,
}
// A nullable field is not an optional receipt: omission must fail closed.
fn required_option<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
/// Implement only through trusted process supervision, not comments or PID absence.
pub trait StoppedOwner {
    fn verify_stopped(&mut self, previous: &Identity, next: &Identity) -> Result<String>;
}
pub struct Store {
    directory: PathBuf,
    _lock: File,
    installation: Identity,
    app: AppIdentity,
}
impl Store {
    /// Explicit one-time operator provisioning. This does not authorize activation.
    pub fn initialize(root: &Path, app: AppIdentity, owner: Identity) -> Result<Self> {
        require(root.is_absolute(), "receipt path must be absolute")?;
        require(
            !root.exists() && !root.is_symlink(),
            "receipt root already exists; do not reinitialize",
        )?;
        let parent = root
            .parent()
            .ok_or_else(|| crate::Error::Stop("missing parent".into()))?;
        let parent = parent.canonicalize()?;
        require(
            !parent.ancestors().any(|p| p.join(".git").exists()),
            "receipts must be outside checkouts",
        )?;
        require(
            !app.app_id.is_empty() && !app.actor_id.is_empty() && !app.login.is_empty(),
            "missing App identity",
        )?;
        DirBuilder::new().mode(0o700).create(root)?;
        File::open(&parent)?.sync_all()?;
        let installation = Identity::new();
        let store = Self::locked(root, installation.clone(), app.clone())?;
        let state = State {
            version: 1,
            installation: installation.clone(),
            app: app.clone(),
            owner,
            handoffs: Vec::new(),
            active: None,
            integrity_stop: None,
            anchors: BTreeMap::new(),
            pending_record: None,
            pending_effect: None,
        };
        store.save(&state)?;
        store.write("identity.json", &(installation, app))?;
        Ok(store)
    }
    pub fn open(root: &Path, next: Identity, verifier: &mut dyn StoppedOwner) -> Result<Self> {
        Self::check_root(root)?;
        let (installation, app): (Identity, AppIdentity) =
            Self::read_private(root, "identity.json")?;
        let store = Self::locked(root, installation, app)?;
        let mut state = store.load()?;
        require(
            state.owner != next,
            "restart needs a fresh controller process identity",
        )?;
        let evidence = verifier.verify_stopped(&state.owner, &next)?;
        require(!evidence.is_empty(), "missing stopped-owner evidence")?;
        if state.active.is_some() {
            state.handoffs.push(OwnerChange {
                previous: state.owner.clone(),
                owner: next.clone(),
                evidence,
            });
        }
        state.owner = next;
        store.save(&state)?;
        Ok(store)
    }
    fn check_root(root: &Path) -> Result<()> {
        require(
            root.is_absolute() && root.canonicalize()? == root,
            "receipt path must be canonical, not symlinked",
        )?;
        let metadata = fs::symlink_metadata(root)?;
        let uid = fs::metadata("/proc/self")?.uid();
        require(
            metadata.is_dir()
                && metadata.uid() == uid
                && metadata.permissions().mode() & 0o077 == 0,
            "receipt root must be owned and private",
        )?;
        require(
            !root.ancestors().any(|p| p.join(".git").exists()),
            "receipts must be outside checkouts",
        )
    }
    fn locked(root: &Path, installation: Identity, app: AppIdentity) -> Result<Self> {
        Self::check_root(root)?;
        let path = root.join("lock");
        require(!path.is_symlink(), "symlinked lock")?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)?;
        Self::check_file(&path)?;
        lock.try_lock()
            .map_err(|_| crate::Error::Stop("controller lock held".into()))?;
        File::open(root)?.sync_all()?;
        Ok(Self {
            directory: root.into(),
            _lock: lock,
            installation,
            app,
        })
    }
    fn check_file(path: &Path) -> Result<()> {
        let m = fs::symlink_metadata(path)?;
        require(
            m.is_file()
                && m.nlink() == 1
                && m.uid() == fs::metadata("/proc/self")?.uid()
                && m.permissions().mode() & 0o077 == 0,
            "receipt file must be owned, private, and unlinked",
        )
    }
    fn read_private<T: serde::de::DeserializeOwned>(root: &Path, name: &str) -> Result<T> {
        let path = root.join(name);
        Self::check_file(&path)?;
        Ok(serde_json::from_reader(File::open(path)?)?)
    }
    pub fn load(&self) -> Result<State> {
        Self::check_root(&self.directory)?;
        let state: State = Self::read_private(&self.directory, "state.json")?;
        require(
            state.version == 1 && state.installation == self.installation && state.app == self.app,
            "lost or mismatched receipt identity",
        )?;
        Ok(state)
    }
    pub(crate) fn save(&self, state: &State) -> Result<()> {
        self.write("state.json", state)
    }
    fn write(&self, name: &str, value: &impl Serialize) -> Result<()> {
        Self::check_root(&self.directory)?;
        let path = self.directory.join(name);
        if path.try_exists()? || path.is_symlink() {
            Self::check_file(&path)?;
        }
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer(&mut file, value)?;
        file.write_all(b"\n")?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|e| crate::Error::Io(e.error))?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}
