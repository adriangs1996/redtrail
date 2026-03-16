use super::super::KnowledgeBase;
use super::super::types::host::HostInfo;

impl KnowledgeBase {
    pub fn host_with_port(&self, ip: &str, port: u16) -> Option<&HostInfo> {
        self.discovered_hosts
            .iter()
            .find(|h| h.ip == ip && h.ports.contains(&port))
    }

    pub fn get_host(&self, ip: &str) -> Option<&HostInfo> {
        self.discovered_hosts.iter().find(|h| h.ip == ip)
    }
}
