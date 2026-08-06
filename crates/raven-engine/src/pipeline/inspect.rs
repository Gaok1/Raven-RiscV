//! What a host sees of a [`ScalarPipeline`], and the knobs it can turn.
//!
//! Written once against the declared shape, so an architecture never writes an
//! observer adapter of its own.

use crate::capability::{
    PipelineControl, PipelineEdge, PipelineEdgeKind, PipelineInspect, PipelineStageView,
    PipelineStats, PipelineStatus, PipelineTimelineCell, PipelineTimelineRow, PipelineTraceView,
    PipelineUnitView,
};

use super::op::PipelineOp;
use super::scalar::ScalarPipeline;

impl<P> PipelineControl for ScalarPipeline<P> {
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn reset(&mut self, address: u64) {
        self.reset_to(address, false);
    }

    fn redirect(&mut self, address: u64) {
        self.reset_to(address, true);
    }
}

impl<P> PipelineInspect for ScalarPipeline<P> {
    fn status(&self) -> PipelineStatus {
        PipelineStatus {
            enabled: self.enabled,
            sequential: !self.enabled,
            halted: self.halted,
            faulted: self.faulted,
        }
    }

    fn stats(&self) -> PipelineStats {
        self.stats
    }

    fn stage_count(&self) -> usize {
        self.shape.stage_count()
    }

    fn stage(&self, index: usize) -> Option<PipelineStageView<'_>> {
        let spec = self.shape.stages.get(index)?;
        Some(PipelineStageView {
            name: spec.name,
            role: spec.role,
            slot: self.stages.get(index)?.as_ref().map(PipelineOp::view),
        })
    }

    /// The straight chain, plus every bypass the shape declares, plus the
    /// redirect a resolved branch takes back to the front end.
    ///
    /// A datapath whose execute stage is last — SAP's three-stage one — declares
    /// no bypasses here, because there is nothing downstream to bypass from.
    fn edges(&self) -> Vec<PipelineEdge> {
        let count = self.shape.stage_count();
        let execute = self.shape.execute_stage();
        let mut edges: Vec<PipelineEdge> = (1..count)
            .map(|to| PipelineEdge::sequential(to - 1, to))
            .collect();
        if self.shape.bypass.forwards_operands() {
            edges.extend((execute + 1..count).map(|from| PipelineEdge {
                from,
                to: execute,
                kind: PipelineEdgeKind::Forward,
            }));
        }
        if count > 1 {
            edges.push(PipelineEdge {
                from: count - 1,
                to: 0,
                kind: PipelineEdgeKind::Feedback,
            });
        }
        edges
    }

    fn unit_count(&self) -> usize {
        self.shape.units.len()
    }

    fn unit(&self, index: usize) -> Option<PipelineUnitView<'_>> {
        let spec = self.shape.units.get(index)?;
        let capacity = self.unit_capacity.get(index).copied().unwrap_or(1);
        // With dispatch on, a unit reports what it is actually holding; without
        // it, the units are a read-only view of the stage they belong to.
        let (active, working) = if self.shape.dispatches_to_units() {
            let bank = self.units.get(index)?;
            // Work still in execute is already this unit's — it is being
            // dispatched, or it finishes in the cycle it started and never has
            // to wait in the bank at all. Counting only the bank would leave a
            // single-cycle ALU reading as permanently idle.
            let executing = self
                .stages
                .get(self.shape.execute_stage())
                .and_then(Option::as_ref)
                .filter(|op| bank.len() < capacity && spec.handles(op.class));
            (
                bank.len() + usize::from(executing.is_some()),
                bank.first().or(executing),
            )
        } else {
            let stage = self
                .shape
                .role_stage(spec.stage)
                .unwrap_or_else(|| self.shape.execute_stage());
            let working = self
                .stages
                .get(stage)
                .and_then(Option::as_ref)
                .filter(|op| spec.handles(op.class));
            (usize::from(working.is_some()), working)
        };
        Some(PipelineUnitView {
            name: spec.name,
            capacity,
            active,
            first: working.map(PipelineOp::view),
            latency_class: working.map_or(spec.latency_class, |op| op.class),
            // The declared timing is right here, so the unit can say what it
            // counts down from rather than leaving a host to guess.
            latency: Some(self.shape.timing.latency(
                working.map_or(spec.latency_class, |op| op.class),
            )),
        })
    }

    fn trace_count(&self) -> usize {
        self.traces.len()
    }

    fn trace(&self, index: usize) -> Option<PipelineTraceView<'_>> {
        let trace = self.traces.get(index)?;
        Some(PipelineTraceView {
            kind: trace.kind,
            from_stage: trace.from,
            to_stage: trace.to,
            detail: &trace.detail,
        })
    }

    fn status_message(&self) -> Option<&str> {
        self.status.as_deref()
    }

    fn timeline_len(&self) -> usize {
        self.timeline.len()
    }

    fn timeline_row(&self, index: usize) -> Option<PipelineTimelineRow<'_>> {
        self.timeline.row(index)
    }

    fn timeline_cell(&self, row: usize, cell: usize) -> Option<PipelineTimelineCell<'_>> {
        self.timeline.cell(row, cell)
    }
}
