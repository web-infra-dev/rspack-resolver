use std::fmt;

use indexmap::IndexMap;

pub use crate::value::owned::Value as OwnedValue;
pub use serde_json as __serde_json;

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
  message: String,
  line: usize,
  column: usize,
}

impl Error {
  pub fn new(message: impl Into<String>) -> Self {
    Self {
      message: message.into(),
      line: 0,
      column: 0,
    }
  }

  pub fn with_location(message: impl Into<String>, line: usize, column: usize) -> Self {
    Self {
      message: message.into(),
      line,
      column,
    }
  }

  pub fn line(&self) -> usize {
    self.line
  }

  pub fn column(&self) -> usize {
    self.column
  }
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.message)
  }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
  fn from(error: serde_json::Error) -> Self {
    let line = error.line();
    let column = error.column();
    if line == 0 && column == 0 {
      Self::new(error.to_string())
    } else {
      Self::with_location(error.to_string(), line, column)
    }
  }
}

pub fn to_owned_value(input: &mut str) -> Result<OwnedValue, Error> {
  let value: serde_json::Value = serde_json::from_str(input)?;
  Ok(value.into())
}

pub fn to_string(value: &OwnedValue) -> Result<String, Error> {
  let serde_value: serde_json::Value = value.clone().into();
  serde_json::to_string(&serde_value).map_err(Error::from)
}

pub mod value {
  pub mod owned {
    use super::super::{Error, IndexMap};
    use std::{borrow::Cow, ops::Index};

    #[derive(Clone, Debug, PartialEq)]
    pub enum Number {
      F64(f64),
      I64(i64),
      U64(u64),
    }

    impl Number {
      pub fn as_f64(&self) -> Option<f64> {
        match self {
          Self::F64(v) => Some(*v),
          Self::I64(v) => Some(*v as f64),
          Self::U64(v) => Some(*v as f64),
        }
      }

      pub fn as_i64(&self) -> Option<i64> {
        match self {
          Self::F64(_) => None,
          Self::I64(v) => Some(*v),
          Self::U64(v) => (*v <= i64::MAX as u64).then_some(*v as i64),
        }
      }

      pub fn as_u64(&self) -> Option<u64> {
        match self {
          Self::F64(_) => None,
          Self::I64(v) => (*v >= 0).then_some(*v as u64),
          Self::U64(v) => Some(*v),
        }
      }
    }

    #[derive(Clone, Debug, PartialEq)]
    pub enum Value {
      Null,
      Bool(bool),
      String(String),
      Static(Cow<'static, str>),
      Array(Vec<Value>),
      Object(Object),
      Number(Number),
    }

    pub type Object = IndexMap<String, Value>;

    impl Default for Value {
      fn default() -> Self {
        Self::Null
      }
    }

    impl Value {
      pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
      }

      pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
      }

      pub fn as_object(&self) -> Option<&Object> {
        match self {
          Self::Object(object) => Some(object),
          _ => None,
        }
      }

      pub fn as_object_mut(&mut self) -> Option<&mut Object> {
        match self {
          Self::Object(object) => Some(object),
          _ => None,
        }
      }

      pub fn as_array(&self) -> Option<&[Value]> {
        match self {
          Self::Array(array) => Some(array),
          _ => None,
        }
      }

      pub fn as_str(&self) -> Option<&str> {
        match self {
          Self::String(value) => Some(value.as_str()),
          Self::Static(value) => Some(value.as_ref()),
          _ => None,
        }
      }

      pub fn as_bool(&self) -> Option<bool> {
        match self {
          Self::Bool(value) => Some(*value),
          _ => None,
        }
      }

      pub fn take(&mut self) -> Value {
        std::mem::replace(self, Value::Null)
      }

      pub fn into_array(self) -> Result<Vec<Value>, Error> {
        match self {
          Self::Array(array) => Ok(array),
          _ => Err(Error::new("value is not an array")),
        }
      }

      pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object().and_then(|object| object.get(key))
      }
    }

    impl Index<&str> for Value {
      type Output = Value;

      fn index(&self, index: &str) -> &Self::Output {
        static NULL: Value = Value::Null;
        self
          .as_object()
          .and_then(|object| object.get(index))
          .unwrap_or(&NULL)
      }
    }

    impl Index<usize> for Value {
      type Output = Value;

      fn index(&self, index: usize) -> &Self::Output {
        static NULL: Value = Value::Null;
        self
          .as_array()
          .and_then(|array| array.get(index))
          .unwrap_or(&NULL)
      }
    }

    impl From<serde_json::Value> for Value {
      fn from(value: serde_json::Value) -> Self {
        match value {
          serde_json::Value::Null => Self::Null,
          serde_json::Value::Bool(v) => Self::Bool(v),
          serde_json::Value::Number(num) => {
            if let Some(v) = num.as_u64() {
              Self::Number(Number::U64(v))
            } else if let Some(v) = num.as_i64() {
              Self::Number(Number::I64(v))
            } else {
              Self::Number(Number::F64(num.as_f64().unwrap_or_default()))
            }
          }
          serde_json::Value::String(s) => Self::String(s),
          serde_json::Value::Array(values) => {
            Self::Array(values.into_iter().map(Value::from).collect())
          }
          serde_json::Value::Object(map) => {
            let mut object = Object::with_capacity(map.len());
            for (key, value) in map {
              object.insert(key, Value::from(value));
            }
            Self::Object(object)
          }
        }
      }
    }

    impl From<Value> for serde_json::Value {
      fn from(value: Value) -> Self {
        match value {
          Value::Null => Self::Null,
          Value::Bool(v) => Self::Bool(v),
          Value::String(s) => Self::String(s),
          Value::Static(s) => Self::String(s.into()),
          Value::Array(values) => {
            Self::Array(values.into_iter().map(serde_json::Value::from).collect())
          }
          Value::Object(map) => {
            let iter = map.into_iter().map(|(k, v)| (k, serde_json::Value::from(v)));
            Self::Object(iter.collect())
          }
          Value::Number(number) => match number {
            Number::F64(v) => serde_json::Number::from_f64(v)
              .map(serde_json::Value::Number)
              .unwrap_or(serde_json::Value::Null),
            Number::I64(v) => serde_json::Value::Number(v.into()),
            Number::U64(v) => serde_json::Value::Number(v.into()),
          },
        }
      }
    }
  }
}

#[macro_export]
macro_rules! json {
  ($($json:tt)+) => {{
    let value = $crate::__serde_json::json!($($json)+);
    $crate::OwnedValue::from(value)
  }};
}
