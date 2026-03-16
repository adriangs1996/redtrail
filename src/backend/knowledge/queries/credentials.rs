use super::super::KnowledgeBase;

impl KnowledgeBase {
    pub fn has_credentials_for(&self, host: &str, service: &str) -> bool {
        self.credentials.iter().any(|c| {
            let host_match = c.host == host || c.host.is_empty();
            let service_match = service.is_empty() || c.service.is_empty() || c.service == service;
            host_match && service_match
        })
    }

    pub fn get_credentials_for(
        &self,
        host: &str,
        service: &str,
    ) -> Option<&crate::types::Credential> {
        self.credentials.iter().find(|c| {
            let host_match = c.host == host || c.host.is_empty();
            let service_match = service.is_empty() || c.service.is_empty() || c.service == service;
            host_match && service_match
        })
    }
}
