use super::host_paths::HostPlatform;
use std::{ffi::OsString, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NormalizedPath {
    pub(super) prefix: OsString,
    pub(super) absolute: bool,
    pub(super) components: Vec<OsString>,
}

impl NormalizedPath {
    pub(super) fn from_path(platform: HostPlatform, path: &Path) -> Self {
        if platform != HostPlatform::Windows {
            return parse_unix_path(path);
        }
        parse_windows_path(path)
    }
}

#[cfg(not(unix))]
fn parse_unix_path(path: &Path) -> NormalizedPath {
    let raw = path.to_string_lossy();
    let value = raw.as_ref();
    let (prefix, absolute, remainder) = if value.starts_with("//") && !value.starts_with("///") {
        (OsString::from("//"), true, &value[2..])
    } else if let Some(stripped) = value.strip_prefix('/') {
        (OsString::from("/"), true, stripped)
    } else {
        (OsString::new(), false, value)
    };

    let mut components = Vec::<OsString>::new();
    for part in remainder.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if components.last().is_some_and(|last| last != "..") {
                    components.pop();
                } else if !absolute {
                    components.push(OsString::from(".."));
                }
            }
            _ => {
                components.push(OsString::from(part));
            }
        }
    }

    NormalizedPath {
        prefix,
        absolute,
        components,
    }
}

#[cfg(unix)]
fn parse_unix_path(path: &Path) -> NormalizedPath {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let bytes = path.as_os_str().as_bytes();
    let (prefix, absolute, remainder) = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        (OsString::from("//"), true, &bytes[2..])
    } else if let Some(stripped) = bytes.strip_prefix(b"/") {
        (OsString::from("/"), true, stripped)
    } else {
        (OsString::new(), false, bytes)
    };

    let mut components = Vec::<OsString>::new();
    for part in remainder.split(|&b| b == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                if components.last().is_some_and(|last| last != "..") {
                    components.pop();
                } else if !absolute {
                    components.push(OsString::from(".."));
                }
            }
            _ => {
                components.push(OsStringExt::from_vec(part.to_vec()));
            }
        }
    }

    NormalizedPath {
        prefix,
        absolute,
        components,
    }
}

fn parse_windows_path(path: &Path) -> NormalizedPath {
    if let Some(raw) = path.to_str() {
        let mut value = raw.replace('\\', "/");
        value.make_ascii_lowercase();

        let (prefix, absolute, remainder) = if let Some(remainder) = value.strip_prefix("//") {
            ("//".to_string(), true, remainder)
        } else if value.as_bytes().get(1) == Some(&b':') {
            let prefix = value[..2].to_string();
            let remainder = &value[2..];
            (prefix, remainder.starts_with('/'), remainder)
        } else if let Some(remainder) = value.strip_prefix('/') {
            ("/".to_string(), true, remainder)
        } else {
            (String::new(), false, value.as_str())
        };

        let mut components = Vec::new();
        for component in remainder.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    if components.last().is_some_and(|last| last != "..") {
                        components.pop();
                    } else if !absolute {
                        components.push(OsString::from(".."));
                    }
                }
                _ => components.push(OsString::from(component)),
            }
        }
        NormalizedPath {
            prefix: OsString::from(prefix),
            absolute,
            components,
        }
    } else {
        NormalizedPath {
            prefix: OsString::new(),
            absolute: false,
            components: vec![path.as_os_str().to_os_string()],
        }
    }
}
