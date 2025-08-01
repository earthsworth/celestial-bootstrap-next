mod resolving;

use crate::java::resolving::resolve_java_version;
use log::error;
use std::path::{Path, PathBuf};

pub trait JdkTrait {
    fn java_executable(&self) -> &Path;
    fn version(&self) -> i32;
}

pub struct Jdk {
    java_executable: PathBuf,
    version: i32,
}

impl Jdk {
    /// Resolve Jdk
    pub async fn resolve_higher(minimalize_version: i32) -> Option<Self> {
        // resolve env PATH
        let java_exec_list = which::which_all_global("java");
        let Ok(java_exec_list) = java_exec_list else {
            error!("Failed to resolve java_exec_list");
            return None;
        };

        // filter java
        for executable in java_exec_list {
            let Ok(file_version) = resolve_java_version(&executable).await else {
                continue; // bad file
            };
            let Some(version): Option<i32> =
                file_version.split(".").next().map(|s| s.parse().unwrap())
            else {
                continue;
            };
            if version >= minimalize_version {
                return Some(Self {
                    java_executable: executable,
                    version,
                });
            }
        }
        None
    }
}

impl JdkTrait for Jdk {
    fn java_executable(&self) -> &Path {
        self.java_executable.as_ref()
    }

    fn version(&self) -> i32 {
        self.version
    }
}
