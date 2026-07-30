#[cfg(test)]
mod tests {
  use crate::{package_json::ParseError, PackageJson, UstrPath};

  #[tokio::test]
  async fn test_json_with_bom() {
    let mock_path = UstrPath::new("package.json");
    let json_with_bom = b"\xEF\xBB\xBF{\"name\": \"example-package\"}".to_vec();

    let result = PackageJson::parse(mock_path, mock_path, json_with_bom).err();

    assert_eq!(
      result,
      Some(ParseError {
        message: "BOM character found".to_string(),
        index: 0
      })
    );
  }

  #[tokio::test]
  async fn test_normal_json() {
    let mock_path = UstrPath::new("package.json");
    let json_with_bom = r##"{"name": "example-package"}"##.as_bytes().to_vec();

    let parsed = PackageJson::parse(mock_path, mock_path, json_with_bom).unwrap();

    assert_eq!(parsed.name.unwrap(), "example-package");
  }

  #[tokio::test]
  async fn test_broken_json() {
    let mock_path = UstrPath::new("package.json");
    let json_with_bom = r##"{"broken":"string"##.as_bytes().to_vec();

    let parsed_err = PackageJson::parse(mock_path, mock_path, json_with_bom).err();

    assert_eq!(
      parsed_err,
      Some(ParseError {
        message: "syntax".to_string(),
        // SIMD error message does not provide the accurate index
        index: 0
      })
    );
  }

  #[tokio::test]
  async fn test_empty_string() {
    let mock_path = UstrPath::new("package.json");
    let json_with_bom = "    ".as_bytes().to_vec();

    let parse_error = PackageJson::parse(mock_path, mock_path, json_with_bom)
      .err()
      .unwrap();

    assert_eq!(
      parse_error,
      ParseError {
        message: "eof".to_string(),
        index: 0,
      }
    );
  }

  #[tokio::test]
  async fn package_json_path_is_one_interned_pointer_across_resolves() {
    use crate::{ResolveOptions, Resolver};

    let f = crate::tests::fixture().join("extensions");
    let resolver = Resolver::new(ResolveOptions {
      extensions: vec![".ts".into(), String::new(), ".js".into()],
      ..ResolveOptions::default()
    });

    let a = resolver.resolve(&f, "./foo").await.expect("should resolve");
    let b = resolver
      .resolve(&f, "./foo.ts")
      .await
      .expect("should resolve");

    let (Some(pa), Some(pb)) = (a.package_json(), b.package_json()) else {
      panic!("both resolutions should carry a package.json");
    };
    assert_eq!(
      pa.path.as_str().as_ptr(),
      pb.path.as_str().as_ptr(),
      "the same package.json path must be one interned pointer, not two copies"
    );
  }
}
