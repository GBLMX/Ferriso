use ferriso_core::{App, Command};
use ferriso_workspace::TableState;

pub const TOTAL_ROWS: usize = 100_000;
pub const ROW_HEIGHT: f32 = 28.0;
pub const DEFAULT_VISIBLE_ROWS: usize = 18;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataRow {
    pub id: usize,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub author: String,
    pub kind: String,
    pub loaders: Vec<String>,
    pub game_versions: Vec<String>,
    pub downloads: u64,
    pub followers: u64,
    pub updated: String,
    pub license: Option<String>,
    pub status: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataRowView {
    pub index: usize,
    pub id: usize,
    pub title: String,
    pub author: String,
    pub kind: String,
    pub loaders: String,
    pub status: String,
    pub downloads: u64,
    pub updated: String,
    pub license: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkbenchView {
    pub rows: Vec<DataRowView>,
    pub selected: Option<usize>,
    pub filtered: usize,
    pub total: usize,
    pub query: String,
    pub status: String,
    pub visible_start: usize,
    pub visible_end: usize,
    pub viewport_start: usize,
    pub viewport_rows: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    QueryChanged(String),
    ClearFilter,
    Move(isize),
    Page(isize),
    First,
    Last,
    Select(usize),
    Viewport(usize),
}

pub struct DataWorkbench {
    rows: Vec<DataRow>,
    filtered: Vec<usize>,
    table: TableState,
    query: String,
    status: String,
}

impl DataWorkbench {
    pub fn new() -> Self {
        Self::with_rows(generate_rows(TOTAL_ROWS))
    }

    pub fn with_rows(rows: Vec<DataRow>) -> Self {
        let total = rows.len();
        let mut table = TableState::new(ROW_HEIGHT, DEFAULT_VISIBLE_ROWS as f32 * ROW_HEIGHT);
        table.set_row_count(total);
        if total > 0 {
            table.select(0);
        }
        let filtered = (0..total).collect();
        Self {
            rows,
            filtered,
            table,
            query: String::new(),
            status: format!("{total} rows loaded"),
        }
    }

    pub fn with_visible_rows(mut self, rows: usize) -> Self {
        self.table
            .set_viewport_height(rows.max(1) as f32 * ROW_HEIGHT);
        self
    }

    fn rebuild_filter(&mut self) {
        let query = self.query.trim().to_ascii_lowercase();
        self.filtered.clear();
        if query.is_empty() {
            self.filtered.extend(0..self.rows.len());
        } else {
            self.filtered.extend(
                self.rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| {
                        row.title.to_ascii_lowercase().contains(&query)
                            || row.slug.to_ascii_lowercase().contains(&query)
                            || row.author.to_ascii_lowercase().contains(&query)
                            || row.summary.to_ascii_lowercase().contains(&query)
                            || row.kind.to_ascii_lowercase().contains(&query)
                            || row.status.to_ascii_lowercase().contains(&query)
                            || row
                                .loaders
                                .iter()
                                .any(|loader| loader.to_ascii_lowercase().contains(&query))
                    })
                    .map(|(index, _)| index),
            );
        }
        self.table.set_row_count(self.filtered.len());
        if self.filtered.is_empty() {
            self.table.clear_selection();
            self.table.set_scroll_offset(0.0);
        } else {
            self.table.select(0);
            self.table.set_scroll_offset(0.0);
        }
        self.status = if query.is_empty() {
            format!("{} rows loaded", self.rows.len())
        } else {
            format!("{} of {} rows match", self.filtered.len(), self.rows.len())
        };
    }
}

impl Default for DataWorkbench {
    fn default() -> Self {
        Self::new()
    }
}

impl App for DataWorkbench {
    type Message = Message;
    type View = WorkbenchView;

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::QueryChanged(query) => {
                if self.query != query {
                    self.query = query;
                    self.rebuild_filter();
                }
            }
            Message::ClearFilter => {
                if !self.query.is_empty() {
                    self.query.clear();
                    self.rebuild_filter();
                }
            }
            Message::Move(delta) => self.table.move_selection(delta),
            Message::Page(direction) => self.table.move_page(direction),
            Message::First => self.table.select_first(),
            Message::Last => self.table.select_last(),
            Message::Select(index) => self.table.select(index),
            Message::Viewport(rows) => {
                self.table
                    .set_viewport_height(rows.max(1) as f32 * ROW_HEIGHT);
                self.table.reveal_selected();
            }
        }
        Command::none()
    }

    fn view(&self) -> Self::View {
        let visible = self.table.visible_range();
        let selected = self.table.selected();
        let viewport_rows = self.table.page_size();
        let viewport_start = (self.table.scroll_offset() / ROW_HEIGHT) as usize;
        let rows = visible
            .clone()
            .filter_map(|position| {
                let source = *self.filtered.get(position)?;
                let row = self.rows.get(source)?;
                Some(DataRowView {
                    index: position,
                    id: row.id,
                    title: row.title.clone(),
                    author: row.author.clone(),
                    kind: row.kind.clone(),
                    loaders: row.loaders.join(", "),
                    status: row.status.clone(),
                    downloads: row.downloads,
                    updated: row.updated.clone(),
                    license: row.license.clone().unwrap_or_else(|| "—".into()),
                    selected: selected == Some(position),
                })
            })
            .collect();
        WorkbenchView {
            rows,
            selected,
            filtered: self.filtered.len(),
            total: self.rows.len(),
            query: self.query.clone(),
            status: self.status.clone(),
            visible_start: visible.start,
            visible_end: visible.end,
            viewport_start,
            viewport_rows,
        }
    }
}

fn generate_rows(count: usize) -> Vec<DataRow> {
    const AUTHORS: [&str; 4] = ["Atlas Studio", "Beacon Labs", "Cinder Works", "Delta Mods"];
    const KINDS: [&str; 4] = ["mod", "modpack", "resourcepack", "shader"];
    const STATUSES: [&str; 4] = ["active", "pending", "archived", "quarantined"];
    const LICENSES: [Option<&str>; 5] = [
        Some("MIT"),
        Some("Apache-2.0"),
        Some("ARR"),
        Some("LGPL-3.0"),
        None,
    ];
    (0..count)
        .map(|index| {
            let author = AUTHORS[index % AUTHORS.len()];
            let kind = KINDS[(index / 3) % KINDS.len()];
            let title = match index % 7 {
                0 => format!("Mekanism 中文整合包 ⚙ {index:05}"),
                1 => format!("Fabric API {index:05}"),
                2 => format!("Sodium 光影增强 {index:05}"),
                _ => format!("{author} {kind} {index:05}"),
            };
            let summary = if index % 11 == 0 {
                String::new()
            } else {
                format!(
                    "Deterministic {kind} fixture with Unicode 中文, tags, and version metadata."
                )
            };
            let loaders = match kind {
                "mod" => vec!["fabric".into(), "forge".into()],
                "modpack" => vec!["fabric".into()],
                "shader" => vec!["iris".into()],
                _ => Vec::new(),
            };
            DataRow {
                id: index,
                slug: format!(
                    "{}-{}",
                    author.to_ascii_lowercase().replace(' ', "-"),
                    index
                ),
                title,
                summary,
                author: author.into(),
                kind: kind.into(),
                loaders,
                game_versions: vec![if index % 2 == 0 {
                    "1.21.1".into()
                } else {
                    "1.20.1".into()
                }],
                downloads: ((index as u64 * 7919) % 5_000_000) + 1,
                followers: ((index as u64 * 37) % 50_000) + 1,
                updated: format!("2026-{:02}-{:02}", 1 + (index % 12), 1 + (index % 28)),
                license: LICENSES[index % LICENSES.len()].map(str::to_string),
                status: STATUSES[(index / AUTHORS.len()) % STATUSES.len()].to_string(),
                size_bytes: ((index as u64 * 104_729) % 2_000_000_000) + 1,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rows() -> Vec<DataRow> {
        (0..50)
            .map(|index| {
                let team = if index % 2 == 0 { "Atlas" } else { "Beacon" };
                DataRow {
                    id: index,
                    slug: format!("sample-{index}"),
                    title: format!("{team} project {index:02}"),
                    summary: if index % 5 == 0 {
                        String::new()
                    } else {
                        format!("Sample {team} summary")
                    },
                    author: team.into(),
                    kind: if index % 4 == 0 { "mod" } else { "modpack" }.into(),
                    loaders: if index % 3 == 0 {
                        vec!["fabric".into()]
                    } else {
                        Vec::new()
                    },
                    game_versions: vec!["1.21.1".into()],
                    downloads: index as u64 * 10,
                    followers: index as u64,
                    updated: "2026-10-28".into(),
                    license: if index % 2 == 0 {
                        Some("MIT".into())
                    } else {
                        None
                    },
                    status: if index % 3 == 0 { "running" } else { "done" }.into(),
                    size_bytes: index as u64 * 1024,
                }
            })
            .collect()
    }

    #[test]
    fn filter_rebuilds_table_and_resets_selection() {
        let mut app = DataWorkbench::with_rows(sample_rows());
        app.update(Message::Select(17));
        app.update(Message::QueryChanged("atlas".into()));

        let view = app.view();
        assert_eq!(view.filtered, 25);
        assert_eq!(view.total, 50);
        assert_eq!(view.selected, Some(0));
        assert!(view.rows.iter().all(|row| row.author == "Atlas"));
        assert_eq!(view.rows[0].license, "MIT");
    }

    #[test]
    fn filters_can_match_loader_metadata() {
        let mut app = DataWorkbench::with_rows(sample_rows());
        app.update(Message::QueryChanged("fabric".into()));
        let view = app.view();
        assert_eq!(view.filtered, 17);
        assert!(view.rows.iter().all(|row| row.loaders.contains("fabric")));
    }

    #[test]
    fn clear_filter_restores_all_rows() {
        let mut app = DataWorkbench::with_rows(sample_rows());
        app.update(Message::QueryChanged("missing".into()));
        assert_eq!(app.view().filtered, 0);
        assert_eq!(app.view().selected, None);

        app.update(Message::ClearFilter);
        let view = app.view();
        assert_eq!(view.filtered, 50);
        assert_eq!(view.selected, Some(0));
    }

    #[test]
    fn view_contains_only_the_visible_window() {
        let app = DataWorkbench::new();
        let view = app.view();
        assert_eq!(view.total, TOTAL_ROWS);
        assert!(view.rows.len() <= 30);
        assert_eq!(view.rows.first().map(|row| row.id), Some(0));
        assert!(view.rows.iter().any(|row| row.title.contains("中文")));
    }

    #[test]
    fn page_and_end_navigation_stay_bounded() {
        let mut app = DataWorkbench::with_rows(sample_rows());
        app.update(Message::Viewport(5));
        app.update(Message::Page(1));
        let selected = app.view().selected.expect("selection");
        assert!(selected > 0);
        app.update(Message::Last);
        assert_eq!(app.view().selected, Some(49));
        app.update(Message::First);
        assert_eq!(app.view().selected, Some(0));
    }

    #[test]
    fn shrinking_viewport_reveals_selected_row() {
        let mut app = DataWorkbench::with_rows(sample_rows());
        app.update(Message::Last);
        app.update(Message::Viewport(3));

        let view = app.view();
        assert_eq!(view.viewport_rows, 3);
        assert_eq!(view.selected, Some(49));
        assert!(view.viewport_start <= 49);
        assert!(view.viewport_start + view.viewport_rows > 49);
        assert!(view.rows.iter().any(|row| row.index == 49));
    }
}
