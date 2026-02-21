use crate::bead::{Bead, BeadStatus, Priority};
use crate::store::BeadStore;

/// Query builder for filtering beads.
pub struct BeadQuery<'a> {
    store: &'a BeadStore,
    prefix: Option<String>,
    status: Option<BeadStatus>,
    assignee: Option<String>,
    label: Option<String>,
    min_priority: Option<Priority>,
    include_closed: bool,
}

impl<'a> BeadQuery<'a> {
    pub fn new(store: &'a BeadStore) -> Self {
        Self {
            store,
            prefix: None,
            status: None,
            assignee: None,
            label: None,
            min_priority: None,
            include_closed: false,
        }
    }

    pub fn prefix(mut self, prefix: &str) -> Self {
        self.prefix = Some(prefix.to_string());
        self
    }

    pub fn status(mut self, status: BeadStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn assignee(mut self, assignee: &str) -> Self {
        self.assignee = Some(assignee.to_string());
        self
    }

    pub fn label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn min_priority(mut self, priority: Priority) -> Self {
        self.min_priority = Some(priority);
        self
    }

    pub fn include_closed(mut self) -> Self {
        self.include_closed = true;
        self
    }

    /// Execute the query, returning matching beads sorted by priority then creation time.
    pub fn execute(self) -> Vec<&'a Bead> {
        let mut results: Vec<&Bead> = self
            .store
            .all()
            .into_iter()
            .filter(|b| {
                if !self.include_closed && b.is_closed() {
                    return false;
                }
                if let Some(ref prefix) = self.prefix {
                    if b.id.prefix() != prefix {
                        return false;
                    }
                }
                if let Some(ref status) = self.status {
                    if &b.status != status {
                        return false;
                    }
                }
                if let Some(ref assignee) = self.assignee {
                    if b.assignee.as_deref() != Some(assignee.as_str()) {
                        return false;
                    }
                }
                if let Some(ref label) = self.label {
                    if !b.labels.contains(label) {
                        return false;
                    }
                }
                if let Some(ref min_pri) = self.min_priority {
                    if b.priority < *min_pri {
                        return false;
                    }
                }
                true
            })
            .collect();

        results.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });

        results
    }
}
