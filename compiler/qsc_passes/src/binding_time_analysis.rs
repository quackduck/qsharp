use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    hir::{CallableDecl, NodeId, Package},
    visit::{walk_package, Visitor},
};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("cannot compare measurement results")]
    #[diagnostic(help(
        "comparing measurement results is not supported for the target runtime capabilities"
    ))]
    #[diagnostic(code("Qsc.BindingTimeAnalysis.ResultComparison"))]
    ResultComparison(#[label] Span),
}

pub fn check_runtime_capabilities(package: &Package) -> Vec<Error> {
    let mut analyzer = Analyzer {
        errors: Vec::new(),
        callable_capabilities: Vec::new(),
    };
    analyzer.visit_package(package);
    analyzer.errors
}

enum RuntimeCapability {
    ConditionalForwardBranching,
    IntegerComputations,
    FloatingPointComputationg,
    BackwardsBranching,
    UserDefinedFunctionCalls,
    HigherLevelConstructs,
}

struct NodeCapabilities {
    id: NodeId,
    capabilities: Vec<RuntimeCapability>,
}

struct CallableCapabilities {
    id: NodeId,
    inherent_capabilities: Vec<NodeCapabilities>,
    parameter_capabilities: Vec<NodeCapabilities>,
}

struct Analyzer {
    errors: Vec<Error>,
    callable_capabilities: Vec<CallableCapabilities>,
}

impl Visitor<'_> for Analyzer {
    fn visit_package(&mut self, package: &'_ Package) {
        if let None = package.entry {
            println!("No entry point");
            return;
        }

        walk_package(self, package);
    }
    fn visit_callable_decl(&mut self, decl: &'_ CallableDecl) {
        let name = decl.name.name.to_string();
        println!("{name}");
    }
}
