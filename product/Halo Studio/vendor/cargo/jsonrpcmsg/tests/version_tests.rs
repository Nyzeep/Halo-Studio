use jsonrpcmsg::version::Version;

#[test]
fn test_version_from_str() {
    assert_eq!(Version::from_str("1.0"), Some(Version::V1_0));
    assert_eq!(Version::from_str("1.1"), Some(Version::V1_1));
    assert_eq!(Version::from_str("2.0"), Some(Version::V2_0));
    assert_eq!(Version::from_str("3.0"), None);
}

#[test]
fn test_version_as_str() {
    assert_eq!(Version::V1_0.as_str(), "1.0");
    assert_eq!(Version::V1_1.as_str(), "1.1");
    assert_eq!(Version::V2_0.as_str(), "2.0");
}

#[test]
fn test_version_display() {
    assert_eq!(format!("{}", Version::V1_0), "1.0");
    assert_eq!(format!("{}", Version::V1_1), "1.1");
    assert_eq!(format!("{}", Version::V2_0), "2.0");
}

#[test]
fn test_version_equality() {
    assert_eq!(Version::V1_0, Version::V1_0);
    assert_ne!(Version::V1_0, Version::V2_0);
}