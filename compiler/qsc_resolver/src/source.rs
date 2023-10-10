pub struct Source {
    pub source: String,
    /// whether or not this module has already had its dependencies inspected
    pub inspected: bool,
}

impl Source {
    pub fn new(raw: String) -> Self {
        Self {
            source: raw,
            inspected: false,
        }
    }
}
