
use super::{
    BranchPredict, BranchResolve, PipelineConfig, PipelineMode, PipelineSpeed,
    parse_pipeline_config, serialize_pipeline_config,
};

#[test]
fn pipeline_config_roundtrip() {
    let cfg = PipelineConfig {
        enabled: false,
        forwarding: false,
        branch_resolve: BranchResolve::Mem,
        mode: PipelineMode::FunctionalUnits,
        predict: BranchPredict::Taken,
        speed: PipelineSpeed::Fast,
    };

    let text = serialize_pipeline_config(&cfg);
    let parsed = parse_pipeline_config(&text).expect("parse pipeline config");
    assert_eq!(parsed, cfg);
}
