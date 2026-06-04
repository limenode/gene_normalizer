use std::sync::Arc;

pub struct TsvStream {
    pub metadata: Vec<String>,
    pub header: Vec<String>,
    pub rows: Box<dyn Iterator<Item = String>>,
}

#[derive(Clone, Debug)]
pub struct GeneRecord {
    pub columns: Arc<[String]>,
    pub values: Vec<String>,
}

impl GeneRecord {
    pub fn get(&self, col: &str) -> Option<&str> {
        let i = self.columns.iter().position(|c| c == col)?;
        self.values.get(i).map(String::as_str)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.columns
            .iter()
            .map(String::as_str)
            .zip(self.values.iter().map(String::as_str))
    }
}
