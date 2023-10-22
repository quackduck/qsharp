use std::collections::HashMap;

use qsc_fir::fir::{LocalItemId, Package};

pub enum RuntimeCapability {
    ConditionalForwardBranching,
    IntegerComputations,
    FloatingPointComputationg,
    BackwardsBranching,
    UserDefinedFunctionCalls,
    HigherLevelConstructs,
}

pub struct PackageRuntimeCapabilities {
    pub items: HashMap<LocalItemId, Vec<RuntimeCapability>>,
}

pub fn analyze_runtime_capabilities(package: &Package) -> PackageRuntimeCapabilities {
    let capabilities = PackageRuntimeCapabilities {
        items: HashMap::new(),
    };
    capabilities
}
