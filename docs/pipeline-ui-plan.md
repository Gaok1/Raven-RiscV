# Pipeline TUI Plan

## Goal

Bring the `Pipeline` tab closer to the `Cache` tab UX:

- visible action buttons inside the TUI
- direct config import/export from the tab itself
- pipeline-specific results export with teaching-oriented metrics

## Current Baseline

The project already had most of the underlying plumbing:

- CLI pipeline config export/import
  - `raven export-pipeline-config`
  - `raven check-pipeline-config`
- TUI keyboard shortcuts for pipeline config
  - `Ctrl+E` exports `.pcfg`
  - `Ctrl+L` imports `.pcfg`
- pipeline runtime metrics already tracked in state
  - committed instructions
  - cycles
  - stalls
  - flushes
  - branch count
  - stall breakdown by type

## UX Changes

### 1. Pipeline controls bar

Add a bottom controls bar to the `Pipeline` tab, mirroring the `Cache` tab pattern.

Buttons:

- `results`
- `import cfg`
- `export cfg`

Behavior:

- `results` should always be visible
- `import cfg` and `export cfg` should appear on the `Config` subtab
- all buttons should support mouse hover and click

### 2. Shortcut discoverability

Expose the shortcuts in the `Pipeline` footer/help:

- `Ctrl+E` export config
- `Ctrl+L` import config
- `Ctrl+R` export results

## Export Formats

### 1. Config export

Keep using the existing `.pcfg` format.

Reason:

- already supported by CLI and TUI
- avoids format duplication
- keeps config files normalized across interfaces

### 2. Pipeline results export

Add pipeline-focused output from the `Pipeline` tab:

- `.pstats`
- `.csv`

Minimum metrics:

- committed
- cycles
- CPI
- stalls
- flushes
- branches
- RAW stalls
- load-use stalls
- branch stalls
- FU stalls
- memory stalls
- forwarding on/off
- mode
- branch resolve policy
- branch predict policy

## Implementation Notes

### Reuse

Prefer reuse over new systems:

- reuse existing `.pcfg` serializer/parser
- reuse existing TUI path-input save/open flow
- reuse existing snapshot/export pattern where practical

### Separation of concerns

- config export/import remains separate from metrics export
- pipeline results should not pretend to be cache snapshots
- cache export can optionally include pipeline data, but pipeline export should stay focused

## Follow-up Opportunities

After the button/export work lands, the next high-value educational improvements are:

1. `Why stalled? / Why flushed?` explanation line
2. pipeline presets for common teaching scenarios
3. more granular forwarding modes
4. comparison/baseline mode for pipeline metrics
5. explicit structural hazard mode later, if complexity is acceptable

## Acceptance Criteria

- `Pipeline` tab shows visible buttons like `Cache`
- `Ctrl+E` / `Ctrl+L` / `Ctrl+R` are documented in the tab UI
- mouse clicks work on the new buttons
- `.pcfg` import/export works from the tab
- results export writes pipeline-focused metrics to file
- project builds and pipeline-related tests still pass
