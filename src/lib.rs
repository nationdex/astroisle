pub mod budget;
pub mod compress;
pub mod config;
pub mod integrity;
pub mod islands;
pub mod module_graph;
pub mod pipeline;
pub mod service_worker;

pub use config::{AstroisleConfig, BudgetConfig, CompressConfig, IntegrityConfig, ServiceWorkerConfig};
pub use pipeline::{Pipeline, PipelineReport};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
