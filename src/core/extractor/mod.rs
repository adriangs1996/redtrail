use inventory;
mod nmap;

pub struct SynthesisResult {}

pub struct Synthetizer {
    runs_on: fn(&str) -> bool,
    stz: fn(&str) -> Result<SynthesisResult, ()>,
}

impl Synthetizer {
    pub const fn new(
        runs_on: fn(&str) -> bool,
        stz: fn(&str) -> Result<SynthesisResult, ()>,
    ) -> Self {
        Self { runs_on, stz }
    }

    pub fn runs_on_tool(&self, tool_name: &str) -> bool {
        (self.runs_on)(tool_name)
    }

    pub fn synthetize(&self, content: &str) -> Result<SynthesisResult, ()> {
        (self.stz)(content)
    }
}

inventory::collect!(Synthetizer);

pub fn synthetize(tool_name: &str, content: &str) -> Result<SynthesisResult, ()> {
    for synthetizer in inventory::iter::<Synthetizer> {
        if synthetizer.runs_on_tool(tool_name) {
            return synthetizer.synthetize(content);
        }
    }

    Err(())
}
