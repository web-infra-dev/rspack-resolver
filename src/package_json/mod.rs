mod simd;

use std::fmt::Display;

pub use simd::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ModuleType {
  #[default]
  CommonJs,
  Module,
}

impl Display for ModuleType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Module => write!(f, "module"),
      Self::CommonJs => write!(f, "commonjs"),
    }
  }
}

impl TryFrom<&str> for ModuleType {
  type Error = &'static str;
  fn try_from(value: &str) -> Result<Self, Self::Error> {
    match value {
      "module" => Ok(Self::Module),
      "commonjs" => Ok(Self::CommonJs),
      _ => Err("Invalid module type"),
    }
  }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideEffects {
  Bool(bool),
  String(String),
  Array(Vec<String>),
}
