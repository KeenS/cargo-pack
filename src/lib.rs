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

use cargo_metadata::{Metadata, MetadataCommand, Package};
use failure::format_err;
use log::debug;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

/// Errors and related
pub mod error {
    /// result type for the cargo-pack
    pub type Result<T> = ::std::result::Result<T, ::failure::Error>;
}

use crate::error::*;

/// Rust side of configurations in `Cargo.toml`
///
/// Cargo.toml will look like
///
/// ```toml
/// [package.metadata.pack]
/// default-packers = ["docker"]
/// files = ["README.md"]
/// ```
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct PackConfig {
    /// files to pack into other than binaries
    pub files: Option<Vec<String>>,
    /// reserved for future usage.
    pub default_packers: Option<Vec<String>>,
}

/// cargo-pack API
pub struct CargoPack {
    package_name: Option<String>,
    pack_config: PackConfig,
    metadata: Metadata,
}

fn lookup(mut value: Value, path: &[&str]) -> Option<Value> {
    for key in path {
        match value {
            Value::Object(mut hm) => match hm.remove(*key) {
                // removing to take the ownership
                Some(v) => value = v,
                None => return None,
            },
            Value::Array(mut v) => match key.parse::<usize>().ok() {
                // NOTICE: may change the index
                Some(idx) if idx < v.len() => value = v.remove(idx),
                _ => return None,
            },
            _ => return None,
        }
    }

    Some(value)
}

impl CargoPack {
    /// create a new CargoPack value
    ///
    /// ```ignore
    /// let config = Config::default().unwrap();
    /// let pack = CargoPack::new(&config, None);
    /// ```

    pub fn new<P: Into<Option<String>>>(package_name: P) -> Result<Self> {
        let package_name = package_name.into();
        let metadata = MetadataCommand::new().no_deps().exec()?;
        let pack_config: PackConfig = Self::decode_from_manifest_static(
            &metadata,
            package_name.as_ref().map(|s| s.as_ref()),
        )?;
        debug!("config: {:?}", pack_config);
        Ok(CargoPack {
            pack_config,
            package_name,
            metadata,
        })
    }

    /// returns the Metadata value
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// returns the PackConfig value
    pub fn config(&self) -> &PackConfig {
        &self.pack_config
    }

    /// returns the `Package` value of `package_name`
    pub fn package(&self) -> Result<&Package> {
        Self::find_package(
            self.metadata(),
            self.package_name.as_ref().map(AsRef::as_ref),
        )
    }

    fn find_package<'a, 'b>(
        metadata: &'a Metadata,
        package_name: Option<&'b str>,
    ) -> Result<&'a Package> {
        if let Some(ref name) = package_name {
            let packages = metadata
                .packages
                .iter()
                .filter(|p| &p.name == name)
                .collect::<Vec<_>>();
            match packages.len() {
                0 => return Err(format_err!("unknown package {}", name)),
                1 => Ok(packages[0]),
                _ => return Err(format_err!("ambiguous name {}", name)),
            }
        } else {
            match metadata.packages.len() {
                1 => Ok(&metadata.packages[0]),
                _ => return Err(format_err!("virtual hogehoge")),
            }
        }
    }

    fn decode_from_manifest_static<T: DeserializeOwned>(
        metadata: &Metadata,
        package_name: Option<&str>,
    ) -> Result<T> {
        let package = Self::find_package(metadata, package_name)?;
        debug!("package: {:?}", package);
        let data = lookup(package.metadata.clone(), &["pack"])
            .expect("no package.metadata.pack found in Cargo.toml");
        serde_json::from_value(data).map_err(Into::into)
    }

    /// decode a value from the manifest toml file.
    pub fn decode_from_manifest<'a, T: DeserializeOwned>(&self) -> Result<T> {
        let package_name = self.package_name.as_ref().map(|s| s.as_ref());
        Self::decode_from_manifest_static(self.metadata(), package_name)
    }

    /// returns files defined in `package.metadata.pack.files` in the Cargo.toml.
    pub fn files(&self) -> &[String] {
        self.pack_config
            .files
            .as_ref()
            .map(AsRef::as_ref)
            .unwrap_or(&[])
    }
}
