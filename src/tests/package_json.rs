#[cfg(test)]
mod tests {
  use std::path::PathBuf;
  use crate::package_json::XParseError;
  use crate::PackageJson;

  #[tokio::test]
  async fn test_json_with_bom() {
    let mock_path = PathBuf::from("package.json");
    let json_with_bom = b"\xEF\xBB\xBF{\"name\": \"example-package\"}".to_vec();

    let result = PackageJson::parse(mock_path.clone(), mock_path.clone(), json_with_bom).err();

    assert_eq!(result, Some(XParseError{
      message: "BOM character found".to_string(),
      index: 0
    }));
  }

  #[tokio::test]
  async fn test_normal_json() {
    let mock_path = PathBuf::from("package.json");
    let json_with_bom = r##"{"name": "example-package"}"##.as_bytes().to_vec();

    let parsed = PackageJson::parse(mock_path.clone(), mock_path.clone(), json_with_bom).unwrap();

    assert_eq!(parsed.name.unwrap(), "example-package");
  }

  #[tokio::test]
  async fn test_broken_json() {
    let mock_path = PathBuf::from("package.json");
    let json_with_bom = r##"{"name":"##.as_bytes().to_vec();

    let parsed_err = PackageJson::parse(mock_path.clone(), mock_path.clone(), json_with_bom).err();

    assert_eq!(parsed_err, Some(XParseError{
        message: "syntax".to_string(),
        index: 7
    }));
  }

  #[tokio::test]
  #[ignore]
  async fn test_empty_string() {
    let mock_path = PathBuf::from("package.json");
    let json_with_bom = "    ".as_bytes().to_vec();

    let parsed = PackageJson::parse(mock_path.clone(), mock_path.clone(), json_with_bom).unwrap();

    assert_eq!(parsed.name.unwrap(), "example-package");
  }
}
