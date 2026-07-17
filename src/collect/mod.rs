pub mod git;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct CollectResult {
    pub items: Vec<crate::model::ActivityItem>,
    pub warnings: Vec<String>,
}

impl CollectResult {
    pub fn merge(&mut self, other: CollectResult) {
        self.items.extend(other.items);
        self.warnings.extend(other.warnings);
    }
}
