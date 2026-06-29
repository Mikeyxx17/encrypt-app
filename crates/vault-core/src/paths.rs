use std::{
    fmt,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::error::{Result, VaultError};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VirtualPath(String);

impl VirtualPath {
    pub fn root() -> Self {
        Self("/".to_string())
    }

    pub fn new(path: impl AsRef<str>) -> Result<Self> {
        let raw = path.as_ref().replace('\\', "/");
        if raw.trim().is_empty() {
            return Err(VaultError::InvalidPath("empty path".to_string()));
        }

        let mut parts = Vec::new();
        for part in raw.split('/') {
            if part.is_empty() || part == "." {
                continue;
            }
            if part == ".." {
                return Err(VaultError::InvalidPath(
                    "parent traversal is not allowed".to_string(),
                ));
            }
            if part.contains('\0') {
                return Err(VaultError::InvalidPath("NUL is not allowed".to_string()));
            }
            parts.push(part.to_string());
        }

        if parts.is_empty() {
            return Ok(Self::root());
        }

        Ok(Self(format!("/{}", parts.join("/"))))
    }

    pub fn from_relative_path(path: &Path) -> Result<Self> {
        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(VaultError::InvalidPath(
                        "parent traversal is not allowed".to_string(),
                    ));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(VaultError::InvalidPath(format!(
                        "absolute paths are not valid vault paths: {}",
                        path.display()
                    )));
                }
            }
        }

        if parts.is_empty() {
            Self::new("/")
        } else {
            Self::new(format!("/{}", parts.join("/")))
        }
    }

    pub fn join(&self, child: impl AsRef<str>) -> Result<Self> {
        if self.0 == "/" {
            Self::new(format!("/{}", child.as_ref()))
        } else {
            Self::new(format!("{}/{}", self.0, child.as_ref()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn file_name(&self) -> Option<&str> {
        self.0.rsplit('/').find(|part| !part.is_empty())
    }

    pub fn parent(&self) -> Option<Self> {
        if self.0 == "/" {
            return None;
        }

        let trimmed = self.0.trim_end_matches('/');
        let Some((parent, _)) = trimmed.rsplit_once('/') else {
            return Some(Self::root());
        };

        if parent.is_empty() {
            Some(Self::root())
        } else {
            Some(Self(parent.to_string()))
        }
    }

    pub fn to_safe_os_path(&self) -> PathBuf {
        let mut path = PathBuf::new();
        for part in self.0.split('/').filter(|part| !part.is_empty()) {
            path.push(sanitize_file_name(part));
        }
        path
    }
}

impl fmt::Display for VirtualPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for VirtualPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

pub fn sanitize_file_name(name: &str) -> String {
    let mut sanitized: String = name
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();

    while sanitized.ends_with(' ') || sanitized.ends_with('.') {
        sanitized.pop();
    }

    if sanitized.is_empty() {
        sanitized = "_".to_string();
    }

    let upper = sanitized.to_ascii_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved.contains(&upper.as_str()) {
        sanitized.push('_');
    }

    sanitized
}
