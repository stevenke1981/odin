use crate::{Fault, Result};
use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub struct SafeRoot {
    root: PathBuf,
    deny: Arc<GlobSet>,
    max_file_bytes: usize,
    directory: Arc<cap_std::fs::Dir>,
}
impl SafeRoot {
    pub fn new(
        root: impl AsRef<Path>,
        deny_globs: &[String],
        max_file_bytes: usize,
    ) -> Result<Self> {
        let root = root.as_ref().canonicalize()?;
        if !root.is_dir() {
            return Err(Fault::invalid("Root is not a directory"));
        }
        let mut builder = GlobSetBuilder::new();
        for pattern in deny_globs {
            builder.add(Glob::new(pattern).map_err(|e| Fault::invalid(e.to_string()))?);
        }
        let directory = Arc::new(cap_std::fs::Dir::open_ambient_dir(
            &root,
            cap_std::ambient_authority(),
        )?);
        Ok(Self {
            root,
            deny: Arc::new(builder.build().map_err(|e| Fault::invalid(e.to_string()))?),
            max_file_bytes,
            directory,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn max_file_bytes(&self) -> usize {
        self.max_file_bytes
    }
    pub fn resolve(&self, relative: &str, allow_missing_leaf: bool) -> Result<PathBuf> {
        if relative.is_empty()
            || relative.contains(['\\', ':', '\0'])
            || Path::new(relative).is_absolute()
        {
            return Err(Fault::denied(
                "Use a nonempty project-relative path with forward slashes",
            ));
        }
        let mut current = self.root.clone();
        let parts: Vec<_> = Path::new(relative).components().collect();
        for (i, component) in parts.iter().enumerate() {
            match component {
                Component::CurDir if parts.len() == 1 => {}
                Component::Normal(part) => {
                    let text = part
                        .to_str()
                        .ok_or_else(|| Fault::invalid("Paths must be UTF-8"))?;
                    let lowered = text.to_ascii_lowercase();
                    let basename = lowered.split('.').next().unwrap_or("");
                    if lowered == ".git"
                        || lowered == ".odin"
                        || text.ends_with([' ', '.'])
                        || [
                            "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5",
                            "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5",
                            "lpt6", "lpt7", "lpt8", "lpt9",
                        ]
                        .contains(&basename)
                    {
                        return Err(Fault::denied("Protected or nonportable path component"));
                    }
                    current.push(part);
                }
                _ => {
                    return Err(Fault::denied(
                        "Parent traversal and platform prefixes are forbidden",
                    ));
                }
            }
            match std::fs::symlink_metadata(&current) {
                Ok(metadata) if is_redirect(&metadata) => {
                    return Err(Fault::denied(
                        "Symbolic links and reparse points are not followed",
                    ));
                }
                Ok(_) => {}
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound
                        && allow_missing_leaf
                        && i + 1 == parts.len() => {}
                Err(e) => return Err(e.into()),
            }
        }
        let rel = current
            .strip_prefix(&self.root)
            .map_err(|_| Fault::denied("Outside project"))?;
        // A deny glob can match a directory as well as a file.
        let mut ancestor = Some(rel);
        while let Some(path) = ancestor {
            if self.deny.is_match(path) {
                return Err(Fault::denied("Path excluded by host policy"));
            }
            ancestor = path.parent().filter(|p| !p.as_os_str().is_empty());
        }
        if current.exists() && !current.canonicalize()?.starts_with(&self.root) {
            return Err(Fault::denied("Resolved path escaped project"));
        }
        Ok(current)
    }
    pub fn read(&self, relative: &str) -> Result<String> {
        self.resolve(relative, false)?;
        let (parent, leaf) = self.parent_handle(relative)?;
        let mut options = cap_std::fs::OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let file = parent.open_with(&leaf, &options)?.into_std();
        if !file.metadata()?.is_file() {
            return Err(Fault::invalid("Expected a regular file"));
        }
        if cap_fs_ext::Metadata::from_file(&file)?.nlink() != 1 {
            return Err(Fault::denied("Hard-linked source files are not authorized"));
        }
        if file.metadata()?.len() > self.max_file_bytes as u64 {
            return Err(Fault::new("LIMIT_EXCEEDED", "File is too large"));
        }
        let mut data = Vec::new();
        file.take(self.max_file_bytes as u64 + 1)
            .read_to_end(&mut data)?;
        if data.len() > self.max_file_bytes {
            return Err(Fault::new("LIMIT_EXCEEDED", "File grew beyond limit"));
        }
        String::from_utf8(data).map_err(|_| Fault::invalid("Only UTF-8 text files are supported"))
    }
    fn parent_handle(&self, relative: &str) -> Result<(cap_std::fs::Dir, std::ffi::OsString)> {
        let path = Path::new(relative);
        let leaf = path
            .file_name()
            .ok_or_else(|| Fault::invalid("Expected a file path"))?
            .to_os_string();
        let mut directory = self.directory.try_clone()?;
        if let Some(parent) = path.parent() {
            for component in parent.components() {
                if let Component::Normal(name) = component {
                    directory = directory.open_dir_nofollow(name)?;
                } else {
                    return Err(Fault::denied("Invalid relative parent"));
                }
            }
        }
        Ok((directory, leaf))
    }
    /// Hold parent directory handles through creation and rename. Each parent
    /// is opened without following links; native source writes use this path.
    pub fn write_atomic(&self, relative: &str, bytes: &[u8]) -> Result<()> {
        self.resolve(relative, true)?;
        if bytes.len() > self.max_file_bytes {
            return Err(Fault::new(
                "LIMIT_EXCEEDED",
                "Replacement exceeds file limit",
            ));
        }
        let (parent, leaf) = self.parent_handle(relative)?;
        let temporary = format!(".odin-tx-{}", uuid::Uuid::new_v4());
        let mut options = cap_std::fs::OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        let result = (|| -> Result<()> {
            let mut file = parent.open_with(&temporary, &options)?;
            if let Ok(metadata) = parent.symlink_metadata(&leaf) {
                if metadata.file_type().is_symlink() {
                    return Err(Fault::denied("Replacement target became a link"));
                }
                file.set_permissions(metadata.permissions())?;
            }
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            parent.rename(&temporary, &parent, &leaf)?;
            #[cfg(unix)]
            parent.try_clone()?.into_std_file().sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = parent.remove_file(&temporary);
        }
        result
    }
    pub fn remove_file(&self, relative: &str) -> Result<()> {
        self.resolve(relative, false)?;
        let (parent, leaf) = self.parent_handle(relative)?;
        parent.remove_file(leaf)?;
        #[cfg(unix)]
        parent.into_std_file().sync_all()?;
        Ok(())
    }
    pub fn relative(&self, absolute: &Path) -> Result<String> {
        let relative = absolute
            .strip_prefix(&self.root)
            .map_err(|_| Fault::denied("External path is not authorized"))?;
        let result = relative
            .to_str()
            .ok_or_else(|| Fault::invalid("Paths must be UTF-8"))?
            .replace('\\', "/");
        self.resolve(&result, false)?;
        Ok(result)
    }
}
/// Replace one file using a same-directory temporary file and rename.
/// This is NOT a multi-file atomic filesystem transaction.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Fault::invalid("Missing parent"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    if let Ok(metadata) = std::fs::metadata(path) {
        tmp.as_file().set_permissions(metadata.permissions())?;
    }
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)
        .map_err(|e| Fault::new("IO_ERROR", e.error.to_string()))?;
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}
/// Create private host-state directories, rejecting existing symlink ancestors.
pub fn private_dir(path: &Path) -> Result<()> {
    let mut ancestor = Some(path);
    while let Some(p) = ancestor {
        if let Ok(meta) = std::fs::symlink_metadata(p)
            && is_redirect(&meta)
        {
            return Err(Fault::denied(
                "State path contains a symlink or reparse point",
            ));
        }
        ancestor = p.parent();
    }
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn is_redirect(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_is_denied() {
        let d = tempfile::tempdir().unwrap();
        let root = SafeRoot::new(d.path(), &[], 1024).unwrap();
        for p in [
            "../x",
            "/etc/passwd",
            "a/../../x",
            "C:/x",
            "a\\b",
            ".git/config",
            "CON.txt",
        ] {
            assert!(root.resolve(p, true).is_err(), "{p}");
        }
    }
    #[test]
    fn missing_parent_is_not_created() {
        let d = tempfile::tempdir().unwrap();
        let root = SafeRoot::new(d.path(), &[], 1024).unwrap();
        assert!(root.resolve("missing/new.txt", true).is_err());
        assert!(root.resolve("new.txt", true).is_ok());
    }
    #[test]
    fn deny_secrets() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(".env"), "SECRET").unwrap();
        let root = SafeRoot::new(d.path(), &["**/.env".into()], 1024).unwrap();
        assert!(root.read(".env").is_err());
    }
    #[test]
    fn limits_and_utf8() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("x"), vec![0xff]).unwrap();
        let root = SafeRoot::new(d.path(), &[], 2).unwrap();
        assert!(root.read("x").is_err());
        std::fs::write(d.path().join("x"), "long").unwrap();
        assert!(root.read("x").is_err());
    }
    #[test]
    fn hardlinked_files_cannot_alias_outside_or_protected_data() {
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("private"), "secret").unwrap();
        std::fs::hard_link(
            outside.path().join("private"),
            directory.path().join("alias"),
        )
        .unwrap();
        let root = SafeRoot::new(directory.path(), &[], 1000).unwrap();
        assert_eq!(root.read("alias").unwrap_err().code, "PERMISSION_DENIED");
    }
    #[test]
    fn handle_relative_write_preserves_content_and_cleans_temporary_files() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("nested")).unwrap();
        let root = SafeRoot::new(directory.path(), &[], 1000).unwrap();
        root.write_atomic("nested/中文.txt", "第一行\r\n".as_bytes())
            .unwrap();
        root.write_atomic("nested/中文.txt", "第二行\r\n".as_bytes())
            .unwrap();
        assert_eq!(root.read("nested/中文.txt").unwrap(), "第二行\r\n");
        assert_eq!(
            std::fs::read_dir(directory.path().join("nested"))
                .unwrap()
                .count(),
            1
        );
        root.remove_file("nested/中文.txt").unwrap();
        assert!(!directory.path().join("nested/中文.txt").exists());
    }
    #[cfg(windows)]
    #[test]
    fn windows_junction_is_denied() {
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let link = directory.path().join("junction");
        let status = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(outside.path())
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        let root = SafeRoot::new(directory.path(), &[], 1000).unwrap();
        assert_eq!(
            root.resolve("junction/data", true).unwrap_err().code,
            "PERMISSION_DENIED"
        );
        std::fs::remove_dir(link).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_denied() {
        let d = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), d.path().join("link")).unwrap();
        let root = SafeRoot::new(d.path(), &[], 1024).unwrap();
        assert!(root.resolve("link/x", true).is_err());
    }
}
