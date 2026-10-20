use crate::{Fault, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::{io::{Read, Write}, path::{Component, Path, PathBuf}, sync::Arc};

#[derive(Clone)]
pub struct SafeRoot {
    root: PathBuf,
    deny: Arc<GlobSet>,
    max_file_bytes: usize,
}
impl SafeRoot {
    pub fn new(root: impl AsRef<Path>, deny_globs: &[String], max_file_bytes: usize) -> Result<Self> {
        let root = root.as_ref().canonicalize()?;
        if !root.is_dir() { return Err(Fault::invalid("Root is not a directory")); }
        let mut builder = GlobSetBuilder::new();
        for pattern in deny_globs {
            builder.add(Glob::new(pattern).map_err(|e| Fault::invalid(e.to_string()))?);
        }
        Ok(Self { root, deny: Arc::new(builder.build().map_err(|e| Fault::invalid(e.to_string()))?), max_file_bytes })
    }
    pub fn root(&self) -> &Path { &self.root }
    pub fn max_file_bytes(&self) -> usize { self.max_file_bytes }
    pub fn resolve(&self, relative: &str, allow_missing_leaf: bool) -> Result<PathBuf> {
        if relative.is_empty() || relative.contains(['\\', ':', '\0']) || Path::new(relative).is_absolute() {
            return Err(Fault::denied("Use a nonempty project-relative path with forward slashes"));
        }
        let mut current = self.root.clone();
        let parts: Vec<_> = Path::new(relative).components().collect();
        for (i, component) in parts.iter().enumerate() {
            match component {
                Component::CurDir if parts.len() == 1 => {},
                Component::Normal(part) => {
                    let text = part.to_str().ok_or_else(|| Fault::invalid("Paths must be UTF-8"))?;
                    let lowered = text.to_ascii_lowercase();
                    let basename = lowered.split('.').next().unwrap_or("");
                    if lowered == ".git" || lowered == ".odin" || text.ends_with([' ', '.'])
                        || ["con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9"].contains(&basename) {
                        return Err(Fault::denied("Protected or nonportable path component"));
                    }
                    current.push(part);
                },
                _ => return Err(Fault::denied("Parent traversal and platform prefixes are forbidden")),
            }
            match std::fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => return Err(Fault::denied("Symbolic links are not followed")),
                Ok(_) => {},
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && allow_missing_leaf && i + 1 == parts.len() => {},
                Err(e) => return Err(e.into()),
            }
        }
        let rel = current.strip_prefix(&self.root).map_err(|_| Fault::denied("Outside project"))?;
        // A deny glob can match a directory as well as a file.
        let mut ancestor = Some(rel);
        while let Some(path) = ancestor {
            if self.deny.is_match(path) { return Err(Fault::denied("Path excluded by host policy")); }
            ancestor = path.parent().filter(|p| !p.as_os_str().is_empty());
        }
        if current.exists() && !current.canonicalize()?.starts_with(&self.root) {
            return Err(Fault::denied("Resolved path escaped project"));
        }
        Ok(current)
    }
    pub fn read(&self, relative: &str) -> Result<String> {
        let path = self.resolve(relative, false)?;
        if !path.is_file() { return Err(Fault::invalid("Expected a regular file")); }
        let file = std::fs::File::open(path)?;
        if file.metadata()?.len() > self.max_file_bytes as u64 { return Err(Fault::new("LIMIT_EXCEEDED", "File is too large")); }
        let mut data = Vec::new();
        file.take(self.max_file_bytes as u64 + 1).read_to_end(&mut data)?;
        if data.len() > self.max_file_bytes { return Err(Fault::new("LIMIT_EXCEEDED", "File grew beyond limit")); }
        String::from_utf8(data).map_err(|_| Fault::invalid("Only UTF-8 text files are supported"))
    }
    pub fn relative(&self, absolute: &Path) -> Result<String> {
        let relative = absolute.strip_prefix(&self.root).map_err(|_| Fault::denied("External path is not authorized"))?;
        let result = relative.to_str().ok_or_else(|| Fault::invalid("Paths must be UTF-8"))?.replace('\\', "/");
        self.resolve(&result, false)?;
        Ok(result)
    }
}
/// Replace one file using a same-directory temporary file and rename.
/// This is NOT a multi-file atomic filesystem transaction.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| Fault::invalid("Missing parent"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    if let Ok(metadata) = std::fs::metadata(path) { tmp.as_file().set_permissions(metadata.permissions())?; }
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| Fault::new("IO_ERROR", e.error.to_string()))?;
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}
/// Create private host-state directories, rejecting existing symlink ancestors.
pub fn private_dir(path: &Path) -> Result<()> {
    let mut ancestor = Some(path);
    while let Some(p) = ancestor {
        if let Ok(meta) = std::fs::symlink_metadata(p) {
            if meta.file_type().is_symlink() { return Err(Fault::denied("State path contains a symlink")); }
        }
        ancestor = p.parent();
    }
    std::fs::create_dir_all(path)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn traversal_is_denied() {
        let d = tempfile::tempdir().unwrap(); let root = SafeRoot::new(d.path(), &[], 1024).unwrap();
        for p in ["../x", "/etc/passwd", "a/../../x", "C:/x", "a\\b", ".git/config", "CON.txt"] {
            assert!(root.resolve(p, true).is_err(), "{p}");
        }
    }
    #[test] fn missing_parent_is_not_created() {
        let d = tempfile::tempdir().unwrap(); let root = SafeRoot::new(d.path(), &[], 1024).unwrap();
        assert!(root.resolve("missing/new.txt", true).is_err());
        assert!(root.resolve("new.txt", true).is_ok());
    }
    #[test] fn deny_secrets() {
        let d = tempfile::tempdir().unwrap(); std::fs::write(d.path().join(".env"), "SECRET").unwrap();
        let root = SafeRoot::new(d.path(), &["**/.env".into()], 1024).unwrap();
        assert!(root.read(".env").is_err());
    }
    #[test] fn limits_and_utf8() {
        let d = tempfile::tempdir().unwrap(); std::fs::write(d.path().join("x"), vec![0xff]).unwrap();
        let root = SafeRoot::new(d.path(), &[], 2).unwrap(); assert!(root.read("x").is_err());
        std::fs::write(d.path().join("x"), "long").unwrap(); assert!(root.read("x").is_err());
    }
    #[cfg(unix)]
    #[test] fn symlink_escape_is_denied() {
        let d = tempfile::tempdir().unwrap(); let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), d.path().join("link")).unwrap();
        let root = SafeRoot::new(d.path(), &[], 1024).unwrap(); assert!(root.resolve("link/x", true).is_err());
    }
}
