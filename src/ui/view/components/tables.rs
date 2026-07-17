//! Tabular helpers: a thin builder over ratatui's real `Table` plus the
//! key/value line helpers the docs pages reach for.
//!
//! Most "tables" in the older views were hand-aligned with `format!("{:<N}")`
//! and duplicated width constants. Prefer:
//!
//! - [`DataTable`] when the data is genuinely columnar and benefits from a
//!   styled header row, zebra striping and per-column alignment.
//! - [`kv_styled`] for the two-column "key: value" blocks (e.g. the Virtual
//!   Memory overview readout).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Cell, Row, Table};

use crate::ui::view::style;

/// Horizontal alignment of a column's cells.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    fn apply(self, line: Line<'static>) -> Line<'static> {
        match self {
            Align::Left => line.alignment(Alignment::Left),
            Align::Center => line.alignment(Alignment::Center),
            Align::Right => line.alignment(Alignment::Right),
        }
    }
}

/// A column definition: header text, a width `Constraint`, and alignment.
pub(crate) struct Col {
    pub header: &'static str,
    pub width: Constraint,
    pub align: Align,
}

impl Col {
    pub(crate) fn new(header: &'static str, width: Constraint, align: Align) -> Self {
        Self {
            header,
            width,
            align,
        }
    }
}

/// A builder over ratatui's `Table` that wires up a styled header row, optional
/// zebra striping and per-column alignment from a `&[Col]` spec.
pub(crate) struct DataTable {
    cols: Vec<Col>,
    rows: Vec<Vec<Line<'static>>>,
    zebra: Option<Color>,
    header: bool,
    block: Option<Block<'static>>,
}

impl DataTable {
    pub(crate) fn new(cols: Vec<Col>) -> Self {
        Self {
            cols,
            rows: Vec::new(),
            zebra: None,
            header: true,
            block: None,
        }
    }

    /// One data row; `cells` must line up with the column spec.
    pub(crate) fn row(mut self, cells: Vec<Line<'static>>) -> Self {
        self.rows.push(cells);
        self
    }

    /// Tint every other row with `bg`.
    pub(crate) fn zebra(mut self, bg: Color) -> Self {
        self.zebra = Some(bg);
        self
    }

    /// Drop the header row (e.g. when the panel title already names the columns).
    pub(crate) fn headerless(mut self) -> Self {
        self.header = false;
        self
    }

    pub(crate) fn block(mut self, block: Block<'static>) -> Self {
        self.block = Some(block);
        self
    }

    pub(crate) fn build(self) -> Table<'static> {
        let widths: Vec<Constraint> = self.cols.iter().map(|c| c.width).collect();
        let aligns: Vec<Align> = self.cols.iter().map(|c| c.align).collect();

        let mut table_rows: Vec<Row<'static>> = Vec::with_capacity(self.rows.len());
        for (i, cells) in self.rows.into_iter().enumerate() {
            let row_cells: Vec<Cell<'static>> = cells
                .into_iter()
                .enumerate()
                .map(|(c, line)| {
                    let align = aligns.get(c).copied().unwrap_or(Align::Left);
                    Cell::from(align.apply(line))
                })
                .collect();
            let mut row = Row::new(row_cells);
            if let Some(bg) = self.zebra.filter(|_| i % 2 == 1) {
                row = row.style(Style::default().bg(bg));
            }
            table_rows.push(row);
        }

        let mut table = Table::new(table_rows, widths);
        if self.header {
            let header_cells: Vec<Cell<'static>> = self
                .cols
                .iter()
                .map(|c| Cell::from(c.align.apply(Line::from(Span::styled(c.header, style::label())))))
                .collect();
            table = table.header(Row::new(header_cells).style(style::label().add_modifier(Modifier::BOLD)));
        }
        if let Some(block) = self.block {
            table = table.block(block);
        }
        table
    }
}

/// A two-column "key: value" block rendered as plain `Line`s; the caller
/// supplies already-styled spans per side and this owns the line + separator.
pub(crate) fn kv_styled(pairs: Vec<(Span<'static>, Span<'static>)>) -> Vec<Line<'static>> {
    pairs
        .into_iter()
        .map(|(k, v)| Line::from(vec![k, Span::raw(" "), v]))
        .collect()
}
