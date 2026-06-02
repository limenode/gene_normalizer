pub struct TsvStream {
    pub metadata: Vec<String>,
    pub header: Vec<String>,
    pub rows: Box<dyn Iterator<Item = String>>,
}
