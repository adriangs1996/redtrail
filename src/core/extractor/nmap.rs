pub(crate) fn extract(content: &str) -> Result<super::SynthesisResult, ()> {
    Ok(super::SynthesisResult {})
}

pub(crate) fn runs_on_tool(tool_name: &str) -> bool {
    tool_name == "nmap"
}

inventory::submit! {
    crate::core::extractor::Synthetizer::new(runs_on_tool, extract)
}
