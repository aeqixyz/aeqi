use crate::quest::{Priority, Quest, QuestStatus};
use crate::store::QuestBoard;

/// Query builder for filtering quests.
pub struct QuestQuery<'a> {
    store: &'a QuestBoard,
    prefix: Option<String>,
    status: Option<QuestStatus>,
    assignee: Option<String>,
    label: Option<String>,
    min_priority: Option<Priority>,
    include_closed: bool,
}

impl<'a> QuestQuery<'a> {
    pub fn new(store: &'a QuestBoard) -> Self {
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

    pub fn status(mut self, status: QuestStatus) -> Self {
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

    /// Execute the query, returning matching quests sorted by priority then creation time.
    pub fn execute(self) -> Vec<&'a Quest> {
        let mut results: Vec<&Quest> = self
            .store
            .all()
            .into_iter()
            .filter(|b| {
                if !self.include_closed && b.is_closed() {
                    return false;
                }
                if let Some(ref prefix) = self.prefix
                    && b.id.prefix() != prefix
                {
                    return false;
                }
                if let Some(ref status) = self.status
                    && &b.status != status
                {
                    return false;
                }
                if let Some(ref assignee) = self.assignee
                    && b.assignee.as_deref() != Some(assignee.as_str())
                {
                    return false;
                }
                if let Some(ref label) = self.label
                    && !b.labels.contains(label)
                {
                    return false;
                }
                if let Some(ref min_pri) = self.min_priority
                    && b.priority < *min_pri
                {
                    return false;
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
