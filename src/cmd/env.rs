//! Env command - display compiler environment variables

use std::collections::HashMap;
use std::path::PathBuf;

pub struct CompilerEnv {
    vars: HashMap<String, PathBuf>,
}

impl CompilerEnv {
    pub fn new() -> Self {
        let mut vars = HashMap::new();
        vars.insert("ROOT".to_string(), resolve_root_path());
        vars.insert("STD_LIB_PATH".to_string(), resolve_std_lib_path());
        vars.insert(
            "THIRD_PARTY_LIB_PATH".to_string(),
            resolve_third_party_lib_path(),
        );
        CompilerEnv { vars }
    }

    pub fn get(&self, key: &str) -> Option<&PathBuf> {
        self.vars.get(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.vars.keys()
    }

    pub fn print(&self) {
        for (key, value) in &self.vars {
            println!("{}={}", key, value.display());
        }
    }

    pub fn print_key(&self, key: &str) -> bool {
        if let Some(value) = self.vars.get(key) {
            println!("{}={}", key, value.display());
            true
        } else {
            false
        }
    }
}

pub fn resolve_std_lib_path() -> PathBuf {
    if let Ok(env_path) = std::env::var("LANG_STD_LIB_PATH") {
        return PathBuf::from(env_path);
    }
    let local_std = PathBuf::from("./std");
    if local_std.exists() {
        return local_std;
    }
    PathBuf::from("/usr/local/lib/lang/std")
}

pub fn resolve_third_party_lib_path() -> PathBuf {
    if let Ok(env_path) = std::env::var("LANG_THIRD_PARTY_LIB_PATH") {
        return PathBuf::from(env_path);
    }
    let local_vendor = PathBuf::from("./vendor");
    if local_vendor.exists() {
        return local_vendor;
    }
    PathBuf::from("/usr/local/lib/lang/vendor")
}

pub fn resolve_root_path() -> PathBuf {
    if let Ok(env_path) = std::env::var("LANG_ROOT") {
        return PathBuf::from(env_path);
    }
    PathBuf::from("/usr/local/lib/lang")
}
