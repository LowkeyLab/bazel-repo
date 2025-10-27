use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct SearchResult {
    pub docs: Vec<Docs>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Docs {
    pub title: String,
    pub author_name: Vec<String>,
    pub first_publish_year: Option<i32>,
    pub key: String,
    pub cover_i: Option<i32>,
    pub subject_key: Vec<String>,
}

pub struct SearchQuery {
    query: String,
}

impl SearchQuery {
    pub fn new(query: String) -> Self {
        SearchQuery { query }
    }
}

#[async_trait::async_trait]
trait OpenLibraryClient {
    async fn search(query: SearchQuery) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>>;
}

pub struct OpenLibraryClientImpl;

#[async_trait::async_trait]
impl OpenLibraryClient for OpenLibraryClientImpl {
    async fn search(query: SearchQuery) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        // Implementation goes here
    }
}

impl SearchResult {
    pub fn new(docs: Vec<Docs>) -> Self {
        SearchResult { docs }
    }

    pub fn cover_url(&self) -> Option<String> {
        self.docs[0]
            .cover_i
            .map(|id| format!("https://covers.openlibrary.org/b/id/{}-M.jpg", id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
