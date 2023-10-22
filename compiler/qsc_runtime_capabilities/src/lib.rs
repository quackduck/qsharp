use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

use qsc_data_structures::index_map::IndexMap;
use qsc_eval::debug::map_hir_package_to_fir;
use qsc_eval::lower::Lowerer;
use qsc_fir::fir::{Ident, ItemKind, LocalItemId, Package, PackageId};
use qsc_frontend::compile::{self};

#[derive(Debug)]
pub enum RuntimeCapability {
    ConditionalForwardBranching,
    QubitReuse,
    IntegerComputations,
    FloatingPointComputationg,
    BackwardsBranching,
    UserDefinedFunctionCalls,
    HigherLevelConstructs,
}

#[derive(Debug)]
pub struct PackageCapabilities {
    pub callables: HashMap<LocalItemId, Vec<RuntimeCapability>>,
}

// DBG: For debugging purposes only.
#[derive(Debug)]
struct AuxPackageData {
    pub callables: HashMap<LocalItemId, Ident>,
}

pub fn analyze_store_capabilities(
    package_store: &compile::PackageStore,
) -> HashMap<PackageId, PackageCapabilities> {
    // Lower to FIR to make it easier to do analysis.
    println!("analyze_store_capabilities");
    let mut fir_lowerer = Lowerer::new();
    let mut fir_store = IndexMap::new();
    for (id, unit) in package_store.iter() {
        fir_store.insert(
            map_hir_package_to_fir(id),
            fir_lowerer.lower_package(&unit.package),
        );
    }

    // DBG: Save FIR store to file for debugging purposes.
    let mut fir_store_file = File::create("dbg/firstore.txt").expect("File could be created");
    let fir_store_string = format!("{:#?}", fir_store);
    write!(fir_store_file, "{}", fir_store_string)
        .expect("Saving FIR store to file should succeed.");

    // DBG: Create an auxiliary data structure for filtered visualization and debugging.
    let mut aux_store: IndexMap<PackageId, AuxPackageData> = IndexMap::new(); // TODO (cesarzc): populate.
    for (id, package) in fir_store.iter() {
        let aux_package_data = create_aux_package_data(package);
        aux_store.insert(id, aux_package_data);
    }

    // DBG: Save the auxiliary data structure to a file for debugging purposes.
    let mut aux_store_file = File::create("dbg/auxstore.txt").expect("File could be created");
    let aux_store_string = format!("{:#?}", aux_store);
    write!(aux_store_file, "{}", aux_store_string)
        .expect("Saving aux store to file should succeed.");

    // Actually do the analysis.
    let mut store_capabilities = HashMap::new();
    for (id, package) in fir_store.iter() {
        let package_capabilities = analyze_package_capabilities(package);
        store_capabilities.insert(id, package_capabilities);
    }
    store_capabilities
}

pub fn analyze_package_capabilities(package: &Package) -> PackageCapabilities {
    let mut capabilities = PackageCapabilities {
        callables: HashMap::new(),
    };

    for (id, item) in package.items.iter() {
        _ = match item.kind {
            ItemKind::Callable(_) => capabilities.callables.insert(id, Vec::new()),
            _ => None,
        }
    }
    capabilities
}

// DBG: For debugging purposes only.
fn create_aux_package_data(package: &Package) -> AuxPackageData {
    let mut aux_package_data = AuxPackageData {
        callables: HashMap::new(),
    };

    for (id, item) in package.items.iter() {
        _ = match &item.kind {
            ItemKind::Callable(callable) => {
                aux_package_data.callables.insert(id, callable.name.clone())
            }
            _ => None,
        }
    }
    aux_package_data
}
