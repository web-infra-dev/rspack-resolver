// mod seder;
// pub use seder::*;

mod simd;
pub use simd::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ModuleType {
  #[default]
  CommonJs,
  Module,
}

impl From<Option<&str>> for ModuleType {
    fn from(value: Option<&str>) -> Self {
        match value {
            Some("module") => ModuleType::Module,
            Some("commonjs") => ModuleType::CommonJs,
            _ => ModuleType::CommonJs,
        }
    }
}
#[derive(Debug,  Clone,  PartialEq, Eq)]
pub enum SideEffects {
    Bool(bool),
    String(String),
    Array(Vec<String>),
}


