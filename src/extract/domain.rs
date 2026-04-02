use super::types::Domain;

/// Map a command binary to its extraction domain.
pub fn detect_domain(binary: &str) -> Domain {
    match binary {
        "git" => Domain::Git,
        "docker" | "docker-compose" | "podman" => Domain::Docker,
        _ => Domain::Generic,
    }
}
