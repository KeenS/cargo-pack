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

pub mod error {
    error_chain!{
        foreign_links {
            Io(::std::io::Error);
            Cargo(Box<::cargo::CargoError>);
        }
    }
}

use error::*;

#[derive(RustcDecodable, Debug)]
pub struct PackConfig {
    files: Vec<String>,
    default_packers: Vec<String>,
}

pub struct CargoPack<'cfg> {
    ws: Workspace<'cfg>,
    package_name: Option<String>,
    pack_config: PackConfig,
}

pub fn lookup(v: Value, path: &str) -> Option<Value> {
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
                    // FIXME: may change index
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
    pub fn ws(&self) -> &Workspace<'cfg> {
        &self.ws
    }
    pub fn config(&self) -> &PackConfig {
        &self.pack_config
    }

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
        let pack_root = lookup(root, "package.metadata.pack").expect("");
        let mut d = Decoder::new(pack_root);
        Ok(Decodable::decode(&mut d).map_err(|e| errors::human(e.to_string()))?)
    }

    pub fn decode_from_manifest<'a, T: Decodable>(&self) -> Result<T> {
        let package_name = self.package_name.as_ref().map(|s| s.as_ref());
        Self::decode_from_manifest_static(self.ws(), package_name)
    }

    pub fn files(&self) -> &[String] {
        self.pack_config.files.as_ref()
    }
}
