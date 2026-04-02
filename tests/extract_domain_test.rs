use redtrail::extract::domain::detect_domain;
use redtrail::extract::types::Domain;

#[test]
fn git_binary() {
    assert_eq!(detect_domain("git"), Domain::Git);
}

#[test]
fn docker_binaries() {
    assert_eq!(detect_domain("docker"), Domain::Docker);
    assert_eq!(detect_domain("docker-compose"), Domain::Docker);
    assert_eq!(detect_domain("podman"), Domain::Docker);
}

#[test]
fn unknown_binary_is_generic() {
    assert_eq!(detect_domain("ls"), Domain::Generic);
    assert_eq!(detect_domain("cat"), Domain::Generic);
    assert_eq!(detect_domain("cargo"), Domain::Generic);
    assert_eq!(detect_domain("npm"), Domain::Generic);
}
