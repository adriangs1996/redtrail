use redtrail::core::db::CommandRow;
use redtrail::extract::docker::DockerExtractor;
use redtrail::extract::types::DomainExtractor;

fn docker_cmd(binary: &str, subcommand: &str, stdout: &str) -> CommandRow {
    CommandRow {
        id: "test-id".into(),
        session_id: "sess".into(),
        command_raw: format!("{binary} {subcommand}"),
        command_binary: Some(binary.into()),
        command_subcommand: Some(subcommand.into()),
        stdout: Some(stdout.into()),
        source: "human".into(),
        timestamp_start: 1000,
        ..Default::default()
    }
}

#[test]
fn can_handle_docker() {
    let ext = DockerExtractor;
    assert!(ext.can_handle("docker", Some("ps")));
    assert!(ext.can_handle("docker-compose", Some("ps")));
    assert!(ext.can_handle("podman", Some("ps")));
    assert!(!ext.can_handle("git", Some("status")));
}

#[test]
fn parse_docker_ps() {
    let stdout = "CONTAINER ID   IMAGE          COMMAND       CREATED        STATUS        PORTS                  NAMES\nabc123def456   nginx:latest   \"nginx -g…\"   2 hours ago    Up 2 hours    0.0.0.0:80->80/tcp     web-server\ndef789abc012   redis:7        \"redis-se…\"   3 hours ago    Up 3 hours    6379/tcp               cache\n";
    let cmd = docker_cmd("docker", "ps", stdout);
    let ext = DockerExtractor;
    let result = ext.extract(&cmd).unwrap();
    let containers: Vec<_> = result
        .entities
        .iter()
        .filter(|e| e.entity_type == "docker_container")
        .collect();
    assert_eq!(containers.len(), 2);
    assert!(containers.iter().any(|c| c.name == "web-server"));
    assert!(containers.iter().any(|c| c.name == "cache"));
}

#[test]
fn parse_docker_images() {
    let stdout = "REPOSITORY   TAG       IMAGE ID       CREATED        SIZE\nnginx        latest    abc123def456   2 weeks ago    187MB\nredis        7         def789abc012   3 weeks ago    130MB\n<none>       <none>    fff000111222   1 month ago    250MB\n";
    let cmd = docker_cmd("docker", "images", stdout);
    let ext = DockerExtractor;
    let result = ext.extract(&cmd).unwrap();
    let images: Vec<_> = result
        .entities
        .iter()
        .filter(|e| e.entity_type == "docker_image")
        .collect();
    assert!(images.len() >= 2); // <none>:<none> may or may not be included
    assert!(images.iter().any(|i| i.name == "nginx:latest"));
}

#[test]
fn parse_docker_build_success() {
    let stdout = "Step 1/5 : FROM node:18\n ---> abc123\nStep 2/5 : COPY . .\n ---> Running in def456\nSuccessfully built abc123def456\nSuccessfully tagged myapp:latest\n";
    let cmd = docker_cmd("docker", "build", stdout);
    let ext = DockerExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(result
        .entities
        .iter()
        .any(|e| e.entity_type == "docker_image" && e.name.contains("myapp")));
}

#[test]
fn empty_stdout_returns_empty() {
    let cmd = docker_cmd("docker", "ps", "");
    let ext = DockerExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(result.is_empty());
}

#[test]
fn parse_docker_ps_container_id_and_image() {
    let stdout = "CONTAINER ID   IMAGE          COMMAND       CREATED        STATUS        PORTS                  NAMES\nabc123def456   nginx:latest   \"nginx -g…\"   2 hours ago    Up 2 hours    0.0.0.0:80->80/tcp     web-server\n";
    let cmd = docker_cmd("docker", "ps", stdout);
    let ext = DockerExtractor;
    let result = ext.extract(&cmd).unwrap();
    let container = result
        .entities
        .iter()
        .find(|e| e.entity_type == "docker_container" && e.name == "web-server")
        .expect("web-server container not found");
    // canonical key should be the container name
    assert_eq!(container.canonical_key, "web-server");
    if let Some(redtrail::extract::types::TypedEntityData::DockerContainer {
        container_id,
        image,
        ..
    }) = &container.typed_data
    {
        assert_eq!(container_id.as_deref(), Some("abc123def456"));
        assert_eq!(image.as_deref(), Some("nginx:latest"));
    } else {
        panic!("expected DockerContainer typed data");
    }
}

#[test]
fn parse_docker_images_canonical_key() {
    let stdout = "REPOSITORY   TAG       IMAGE ID       CREATED        SIZE\nnginx        latest    abc123def456   2 weeks ago    187MB\n";
    let cmd = docker_cmd("docker", "images", stdout);
    let ext = DockerExtractor;
    let result = ext.extract(&cmd).unwrap();
    let image = result
        .entities
        .iter()
        .find(|e| e.entity_type == "docker_image")
        .expect("no docker_image entity");
    assert_eq!(image.canonical_key, "nginx:latest");
    assert_eq!(image.name, "nginx:latest");
}

#[test]
fn parse_docker_compose_ps() {
    let stdout = "Name              Command               State           Ports\n----------------------------------------------------------------------\napp_web_1         nginx -g daemon off;             Up      0.0.0.0:80->80/tcp\napp_db_1          docker-entrypoint.sh postgres    Up      5432/tcp\n";
    let cmd = docker_cmd("docker-compose", "ps", stdout);
    let ext = DockerExtractor;
    let result = ext.extract(&cmd).unwrap();
    let services: Vec<_> = result
        .entities
        .iter()
        .filter(|e| e.entity_type == "docker_service")
        .collect();
    assert!(services.len() >= 1);
    assert!(services.iter().any(|s| s.name == "app_web_1"));
}

#[test]
fn parse_docker_build_id_only() {
    let stdout = "Successfully built deadbeef1234\n";
    let cmd = docker_cmd("docker", "build", stdout);
    let ext = DockerExtractor;
    let result = ext.extract(&cmd).unwrap();
    let images: Vec<_> = result
        .entities
        .iter()
        .filter(|e| e.entity_type == "docker_image")
        .collect();
    assert_eq!(images.len(), 1);
    assert!(images[0].name.contains("deadbeef1234"));
}

#[test]
fn parse_docker_ps_only_header_returns_empty() {
    let stdout =
        "CONTAINER ID   IMAGE   COMMAND   CREATED   STATUS   PORTS   NAMES\n";
    let cmd = docker_cmd("docker", "ps", stdout);
    let ext = DockerExtractor;
    let result = ext.extract(&cmd).unwrap();
    let containers: Vec<_> = result
        .entities
        .iter()
        .filter(|e| e.entity_type == "docker_container")
        .collect();
    assert!(containers.is_empty());
}
