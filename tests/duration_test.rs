use redtrail::core::capture;

#[test]
fn parse_hours() {
    assert_eq!(capture::parse_duration("1h").unwrap(), 3600);
    assert_eq!(capture::parse_duration("2h").unwrap(), 7200);
}

#[test]
fn parse_minutes() {
    assert_eq!(capture::parse_duration("30m").unwrap(), 1800);
    assert_eq!(capture::parse_duration("5m").unwrap(), 300);
}

#[test]
fn parse_seconds() {
    assert_eq!(capture::parse_duration("60s").unwrap(), 60);
}

#[test]
fn parse_days() {
    assert_eq!(capture::parse_duration("7d").unwrap(), 604800);
}

#[test]
fn parse_plain_number_as_seconds() {
    assert_eq!(capture::parse_duration("3600").unwrap(), 3600);
}

#[test]
fn parse_invalid_returns_error() {
    assert!(capture::parse_duration("abc").is_err());
    assert!(capture::parse_duration("").is_err());
    assert!(capture::parse_duration("5x").is_err());
}
