//! an infrastructure library for 'cargo-pack'ers.
//! This crate provides only common features of `pack`ers, currently, files to package.
//! Currently, you can write these metadata in Cargo.toml:
//!
//! ```toml
//! [package.metadata.pack]
//! # Not used for now. Reserved for future use
//! default-packers = ["docker"]
//! # files to pack in addition to binaries
//! files = ["README.md"]
//! ```


#![deny(missing_docs)]
extern crate cargo;
extern crate rustc_serialize;
extern crate toml as toml_crate;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;


use toml_crate::{Decoder, Value};
use cargo::core::Workspace;
use cargo::core::Package;
use cargo::util::important_paths::find_root_manifest_for_wd;
use cargo::util::Config;
use cargo::util::{toml, paths, errors};
use rustc_serialize::Decodable;

/// Errors and related
pub mod error {
    error_chain!{
        foreign_links {
            Io(::std::io::Error)
                /// IO errors
                ;
            Cargo(Box<::cargo::CargoError>)
                /// Erros from the `cargo` crate
                ;
        }
    }
}

use error::*;


/// Rust side of configurations in `Cargo.toml`
///
/// Cargo.toml will look like
///
/// ```toml
/// [package.metadata.pack]
/// default-packers = ["docker"]
/// files = ["README.md"]
/// ```
#[derive(RustcDecodable, Debug)]
pub struct PackConfig {
    /// files to pack into other than binaries
    pub files: Vec<String>,
    /// reserved for future usage.
    pub default_packers: Vec<String>,
}

/// cargo-pack API
pub struct CargoPack<'cfg> {
    ws: Workspace<'cfg>,
    package_name: Option<String>,
    pack_config: PackConfig,
}

fn lookup(v: Value, path: &str) -> Option<Value> {
    let ref path = match toml_crate::Parser::new(path).lookup() {
        Some(path) => path,
        None => return None,
    };
    let mut cur_value = v;
    if path.is_empty() {
        return Some(cur_value);
    }

    for key in path {
        match cur_value {
            Value::Table(mut hm) => {
                // removing to take the ownership
                match hm.remove(key) {
                    Some(v) => cur_value = v,
                    None => return None,
                }
            }
            Value::Array(mut v) => {
                match key.parse::<usize>().ok() {
                    // NOTICE: may change the index
                    Some(idx) if idx < v.len() => cur_value = v.remove(idx),
                    _ => return None,
                }
            }
            _ => return None,
        }
    }

    Some(cur_value)

}


impl<'cfg> CargoPack<'cfg> {
    /// create a new CargoPack value
    ///
    /// ```rust
    /// let config = Config::default().unwrap();
    /// let pack = CargoPack::new(&config, None);
    /// ```

    pub fn new<'a, P: Into<Option<String>>>(config: &'cfg Config, package_name: P) -> Result<Self> {
        let package_name = package_name.into();
        let root = find_root_manifest_for_wd(None, config.cwd())?;
        let ws: Workspace<'cfg> = Workspace::new(&root, config)?;
        let pack_config: PackConfig = Self::decode_from_manifest_static(&ws,
                                                                        package_name.as_ref()
                                                                            .map(|s| s.as_ref()))?;
        debug!("config: {:?}", pack_config);
        Ok(CargoPack {
            ws: ws,
            pack_config: pack_config,
            package_name: package_name,
        })
    }

    /// returns the current working space of the package of `package_name`
    pub fn ws(&self) -> &Workspace<'cfg> {
        &self.ws
    }

    /// returns the PackConfig value
    pub fn config(&self) -> &PackConfig {
        &self.pack_config
    }

    /// returns the `Package` value of `package_name`
    pub fn package(&self) -> Result<&Package> {
        if let Some(ref name) = self.package_name {
            let packages = self.ws()
                .members()
                .filter(|p| p.package_id().name() == *name)
                .collect::<Vec<_>>();
            match packages.len() {
                0 => return Err(format!("unknown package {}", name).into()),
                1 => Ok(packages[0]),
                _ => return Err(format!("ambiguous name {}", name).into()),
            }
        } else {
            Ok(self.ws().current()?)
        }
    }

    fn decode_from_manifest_static<T: Decodable>(ws: &Workspace,
                                                 package_name: Option<&str>)
                                                 -> Result<T> {
        let manifest = if let Some(ref name) = package_name {
            let names = ws.members().filter(|p| p.package_id().name() == *name).collect::<Vec<_>>();
            match names.len() {
                0 => return Err(format!("unknown package {}", name).into()),
                1 => names[0].manifest_path(),
                _ => return Err(format!("ambiguous name {}", name).into()),
            }
        } else {
            ws.current()?.manifest_path()
        };
        debug!("reading manifest: {:?}", manifest);

        let contents = paths::read(manifest)?;
        let root = toml::parse(&contents, &manifest, ws.config())?;
        let root = Value::Table(root);
        debug!("root: {:?}", root);
        let pack_root = lookup(root, "package.metadata.pack")
            .expect("no package.metadate.pack found in Cargo.toml");
        let mut d = Decoder::new(pack_root);
        Ok(Decodable::decode(&mut d).map_err(|e| errors::human(e.to_string()))?)
    }

    /// decode a value from the manifest toml file.
    pub fn decode_from_manifest<'a, T: Decodable>(&self) -> Result<T> {
        let package_name = self.package_name.as_ref().map(|s| s.as_ref());
        Self::decode_from_manifest_static(self.ws(), package_name)
    }

    /// returns files defined in `package.metadata.pack.files` in the Cargo.toml.
    pub fn files(&self) -> &[String] {
        self.pack_config.files.as_ref()
    }
}
